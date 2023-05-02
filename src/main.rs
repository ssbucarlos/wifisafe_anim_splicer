use anyhow::{Context, Result};
use clap::Parser;
use ssbh_lib::{formats::anim::GroupType, prelude::*, SsbhArray, SsbhByteBuffer};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
struct Args {
    #[arg(short = 'r', long)]
    reference_anim_file: Option<PathBuf>,
    #[arg(short = 'm', long)]
    modified_anim_file: Option<PathBuf>,
    #[arg(short = 'o', long)]
    output_file: Option<PathBuf>,
    #[arg(long = "reference_folder")]
    batch_reference_folder: Option<PathBuf>,
    #[arg(long = "modified_folder")]
    batch_modified_folder: Option<PathBuf>,
    #[arg(long = "output_folder")]
    batch_output_folder: Option<PathBuf>,
}

fn splice_anim(reference_anim: &PathBuf, modified_anim: &PathBuf) -> Result<Anim> {
    let reference_anim =
        ssbh_lib::formats::anim::Anim::from_file(reference_anim).with_context(|| {
            format!(
                "could not read reference anim `{}`",
                &reference_anim.display()
            )
        })?;
    let modified_anim =
        ssbh_lib::formats::anim::Anim::from_file(modified_anim).with_context(|| {
            format!(
                "could not read modified anim `{}`",
                &modified_anim.display()
            )
        })?;

    // Validate Anim Versions
    if let Anim::V12 { .. } = &reference_anim {
        return Err(anyhow::format_err!("v12 reference anim not supported!"));
    }
    if let Anim::V12 { .. } = &modified_anim {
        return Err(anyhow::format_err!("v12 modified anim not supported!"));
    }

    // Gather reference data
    let mut reference_node_name_to_buffer = std::collections::HashMap::new();
    let mut reference_node_name_to_track = std::collections::HashMap::new();
    let mut reference_node_names = Vec::new();
    if let Anim::V21 { groups, buffer, .. } | Anim::V20 { groups, buffer, .. } = &reference_anim {
        for group in &groups.elements {
            if group.group_type != GroupType::Transform {
                continue;
            }
            for node in &group.nodes.elements {
                let track = &node.tracks.elements[0];
                let node_name = String::from(node.name.to_str().unwrap());
                let start_index = track.data_offset as usize;
                let end_index = (track.data_offset as u64 + track.data_size) as usize;
                let buffer_slice = &buffer.elements[start_index..end_index];
                reference_node_name_to_buffer.insert(node_name.clone(), buffer_slice);
                reference_node_name_to_track.insert(node_name.clone(), track);
                reference_node_names.push(node.name.clone());
            }
        }
    }

    // Find out and keep track which groups exist in the modified animation
    // A user may provide only the vis track for instance, but the expected
    // result is that the existing transform and mat tracks would remain in
    // the spliced anim.
    let mut modified_group_types: Vec<GroupType> = Vec::new();
    if let Anim::V21 { groups, .. } | Anim::V20 { groups, .. } = &modified_anim {
        for group in &groups.elements {
            modified_group_types.push(group.group_type);
        }
    }

    // Now format the new buffer
    let mut current_offset: u64 = 0;
    let mut new_buffer = SsbhByteBuffer::new();
    let mut new_groups: SsbhArray<ssbh_lib::formats::anim::Group> = SsbhArray::new();
    // First just go through the vanilla anim's bone data and just copy all those buffers as-is.
    // Then we can just grab any new bone data and the vis/mat tracks from the modified anim
    let mut new_transform_group = ssbh_lib::formats::anim::Group {
        group_type: ssbh_lib::formats::anim::GroupType::Transform,
        nodes: SsbhArray::new(),
    };
    if let Anim::V20 { groups, .. } | Anim::V21 { groups, .. } = &reference_anim {
        for reference_group in &groups.elements {
            if reference_group.group_type != GroupType::Transform {
                continue;
            }
            for reference_node in &reference_group.nodes.elements {
                let mut new_node = ssbh_lib::formats::anim::Node {
                    name: reference_node.name.clone(),
                    tracks: SsbhArray::new(),
                };
                for reference_track in &reference_node.tracks.elements {
                    let new_track = ssbh_lib::formats::anim::TrackV2 {
                        data_offset: current_offset as u32,
                        ..reference_track.clone()
                    };
                    let reference_buffer = reference_node_name_to_buffer
                        .get(reference_node.name.to_str().unwrap())
                        .unwrap();
                    new_buffer.elements.extend_from_slice(reference_buffer);
                    current_offset += reference_track.data_size;
                    new_node.tracks.elements.push(new_track);
                }
                new_transform_group.nodes.elements.push(new_node);
            }
        }
    }

    // At this point, only one transform group has been made.
    // Gather new bone data to finish the new transform group
    if let Anim::V20 { groups, buffer, .. } | Anim::V21 { groups, buffer, .. } = &modified_anim {
        for modified_group in &groups.elements {
            if modified_group.group_type != GroupType::Transform {
                continue;
            }
            for modified_node in &modified_group.nodes.elements {
                if reference_node_names.contains(&modified_node.name) {
                    continue;
                }
                let mut new_node = ssbh_lib::formats::anim::Node {
                    name: modified_node.name.clone(),
                    tracks: SsbhArray::new(),
                };
                for modified_track in &modified_node.tracks.elements {
                    let new_track = ssbh_lib::formats::anim::TrackV2 {
                        data_offset: current_offset as u32,
                        ..modified_track.clone()
                    };
                    let start_index = modified_track.data_offset as usize;
                    let end_index =
                        (modified_track.data_offset as u64 + modified_track.data_size) as usize;
                    let modified_buffer = &buffer.elements[start_index..end_index];
                    new_buffer.elements.extend_from_slice(modified_buffer);
                    current_offset += modified_track.data_size;
                    new_node.tracks.elements.push(new_track);
                }
                new_transform_group.nodes.elements.push(new_node);
            }
        }
    }
    new_groups.elements.push(new_transform_group);

    // Now we need to grab the mat/vis groups from the modified anim
    if let Anim::V20 { groups, buffer, .. } | Anim::V21 { groups, buffer, .. } = &modified_anim {
        for modified_group in &groups.elements {
            if modified_group.group_type == GroupType::Transform {
                continue;
            }
            let mut new_group = ssbh_lib::formats::anim::Group {
                group_type: modified_group.group_type,
                nodes: SsbhArray::new(),
            };
            for modified_node in &modified_group.nodes.elements {
                let mut new_node = ssbh_lib::formats::anim::Node {
                    name: modified_node.name.clone(),
                    tracks: SsbhArray::new(),
                };
                for modified_track in &modified_node.tracks.elements {
                    let new_track = ssbh_lib::formats::anim::TrackV2 {
                        data_offset: current_offset as u32,
                        ..modified_track.clone()
                    };
                    let start_index = modified_track.data_offset as usize;
                    let end_index =
                        (modified_track.data_offset as u64 + modified_track.data_size) as usize;
                    let modified_buffer = &buffer.elements[start_index..end_index];
                    new_buffer.elements.extend_from_slice(modified_buffer);
                    current_offset += modified_track.data_size;
                    new_node.tracks.elements.push(new_track);
                }
                new_group.nodes.elements.push(new_node);
            }
            new_groups.elements.push(new_group);
        }
    }

    // Now account for a case where the modified anim only contains one of either the Vis or Mat group,
    // so the reference anim may contain the other group.
    if let Anim::V20 { groups, .. } | Anim::V21 { groups, .. } = &reference_anim {
        for reference_group in &groups.elements {
            if reference_group.group_type == GroupType::Transform {
                continue;
            }
            if modified_group_types.contains(&reference_group.group_type) {
                continue;
            }
            let mut new_group = ssbh_lib::formats::anim::Group {
                group_type: reference_group.group_type,
                nodes: SsbhArray::new(),
            };
            for reference_node in &reference_group.nodes.elements {
                let mut new_node = ssbh_lib::formats::anim::Node {
                    name: reference_node.name.clone(),
                    tracks: SsbhArray::new(),
                };
                for reference_track in &reference_node.tracks.elements {
                    let new_track = ssbh_lib::formats::anim::TrackV2 {
                        data_offset: current_offset as u32,
                        ..reference_track.clone()
                    };
                    let reference_buffer = reference_node_name_to_buffer
                        .get(reference_node.name.to_str().unwrap())
                        .unwrap();
                    new_buffer.elements.extend_from_slice(reference_buffer);
                    current_offset += reference_track.data_size;
                    new_node.tracks.elements.push(new_track);
                }
                new_group.nodes.elements.push(new_node);
            }
            new_groups.elements.push(new_group);
        }
    }

    match reference_anim {
        Anim::V20 {
            final_frame_index,
            unk1,
            unk2,
            name,
            ..
        } => Ok(Anim::V20 {
            final_frame_index,
            unk1,
            unk2,
            name,
            groups: new_groups,
            buffer: new_buffer,
        }),

        Anim::V21 {
            final_frame_index,
            unk1,
            unk2,
            name,
            unk_data,
            ..
        } => Ok(Anim::V21 {
            final_frame_index,
            unk1,
            unk2,
            name,
            groups: new_groups,
            buffer: new_buffer,
            unk_data,
        }),

        _ => Err(anyhow::format_err!(
            "Got an unsupported reference anim but this code should have never been reached "
        )),
    }
}

fn do_batch_mode(
    batch_reference_dir: &PathBuf,
    batch_modified_dir: &PathBuf,
    batch_output_dir: &Path,
) -> Result<()> {
    let reference_anim_paths = fs::read_dir(batch_reference_dir)
        .unwrap()
        .filter_map(|dir_entry| dir_entry.ok())
        .map(|dir_entry| dir_entry.path())
        .filter(|path| path.extension().unwrap().eq("nuanmb"))
        .collect::<Vec<_>>();

    let modified_anim_paths = fs::read_dir(batch_modified_dir)
        .unwrap()
        .filter_map(|dir_entry| dir_entry.ok())
        .map(|dir_entry| dir_entry.path())
        .filter(|path| path.extension().unwrap().eq("nuanmb"))
        .collect::<Vec<_>>();

    for modified_anim_path in modified_anim_paths {
        let matching_vanilla_anim_path: PathBuf = match reference_anim_paths
            .iter()
            .find(|&p| p.file_name() == modified_anim_path.file_name())
        {
            Some(path) => path.clone(),
            None => {
                println!(
                    "Skipping modified file {modified_anim_path:?}, no vanilla anim was found!"
                );
                continue;
            }
        };

        let new_anim: Anim = match splice_anim(&matching_vanilla_anim_path, &modified_anim_path) {
            Ok(anim) => anim,
            Err(e) => {
                println!("An error {e} happened splicing {modified_anim_path:?} with {matching_vanilla_anim_path:?}, so no spliced anim will be outputted.");
                continue;
            }
        };

        let output_file_path = batch_output_dir.join(modified_anim_path.file_name().unwrap());
        new_anim.write_to_file(&output_file_path).with_context(|| {
            format!(
                "could not output the new anim to the output path `{}`",
                &output_file_path.display()
            )
        })?;
    }
    Ok(())
}

fn do_single_mode(
    reference_anim: &PathBuf,
    modified_anim: &PathBuf,
    output_anim: &PathBuf,
) -> Result<()> {
    let new_anim = splice_anim(reference_anim, modified_anim);
    new_anim?.write_to_file(output_anim).with_context(|| {
        format!(
            "could not output the new anim to the output path `{}`",
            &output_anim.display()
        )
    })?;
    Ok(())
}

#[derive(PartialEq)]
enum Mode {
    Single,
    Batch,
    None,
}

fn get_mode(args: &Args) -> Mode {
    if args.batch_reference_folder.is_some()
        || args.batch_modified_folder.is_some()
        || args.batch_output_folder.is_some()
    {
        Mode::Batch
    } else if args.reference_anim_file.is_some()
        || args.modified_anim_file.is_some()
        || args.output_file.is_some()
    {
        Mode::Single
    } else {
        Mode::None
    }
}
fn main() -> Result<()> {
    let start_time = Instant::now();

    let args = Args::parse();

    let mode = get_mode(&args);

    let result = match mode {
        Mode::Batch => {
            let batch_reference_dir = args
                .batch_reference_folder
                .expect("Batch mode specified, but the reference folder was not given!");
            let batch_modified_dir = args
                .batch_modified_folder
                .expect("Batch mode specified, but modified folder is missing!");
            let batch_output_dir = args
                .batch_output_folder
                .expect("Batch mode specified, but the output folder is missing!");
            do_batch_mode(&batch_reference_dir, &batch_modified_dir, &batch_output_dir)
        }
        Mode::Single => {
            let reference_anim_path = args
                .reference_anim_file
                .expect("Batch mode was not specified, but a reference anim was not given!");
            let modified_anim_path = args
                .modified_anim_file
                .expect("Batch mode was not specified, but a modified anim was not given!");
            let output_file_path = args
                .output_file
                .expect("Batch mode was not specified, but the output file path was not provided!");
            do_single_mode(&reference_anim_path, &modified_anim_path, &output_file_path)
        }
        Mode::None => Err(anyhow::format_err!(
            "No arguments passed in! Please run with -h or --help for help."
        )),
    };
    if mode != Mode::None {
        println!("Done! elapsed time = {:?}!", start_time.elapsed());
    }
    result
}
