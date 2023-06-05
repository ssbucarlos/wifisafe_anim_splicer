use anyhow::{Context, Result};
use clap::Parser;
use itertools::Itertools;
use ssbh_lib::formats::anim::{Group, GroupType, Node, TrackV2};
use ssbh_lib::{prelude::*, SsbhArray, SsbhByteBuffer};
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

#[derive(Clone)]
struct AnimTransformNodeData {
    name: String,
    buffer: Vec<u8>,
    track: TrackV2,
}

impl AnimTransformNodeData {
    pub fn from(node: &Node, buffer: &SsbhByteBuffer) -> Self {
        let track = &node.tracks.elements[0];
        let start_index = track.data_offset as usize;
        let end_index = (track.data_offset as u64 + track.data_size) as usize;
        let buffer_slice = &buffer.elements[start_index..end_index];
        Self {
            name: String::from(node.name.to_str().unwrap()),
            buffer: buffer_slice.to_vec(),
            track: track.clone(),
        }
    }
}

struct AnimGroupWithBuffer<'a> {
    group: &'a Group,
    buffer: &'a SsbhByteBuffer,
}

fn get_anim_group_and_buffer_with_fallback<'a>(
    priority_groups: &'a SsbhArray<Group>,
    priority_buffer: &'a SsbhByteBuffer,
    fallback_groups: &'a SsbhArray<Group>,
    fallback_buffer: &'a SsbhByteBuffer,
    group_type: GroupType,
) -> Option<AnimGroupWithBuffer<'a>> {
    let priority_group = priority_groups
        .elements
        .iter()
        .find(|group_entry| group_entry.group_type == group_type);

    let fallback_group = fallback_groups
        .elements
        .iter()
        .find(|group_entry| group_entry.group_type == group_type);

    match priority_group {
        Some(priority_group) => Some(AnimGroupWithBuffer {
            group: priority_group,
            buffer: priority_buffer,
        }),
        None => fallback_group.map(|fallback_group| AnimGroupWithBuffer {
            group: fallback_group,
            buffer: fallback_buffer,
        }),
    }
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

    let (reference_groups, reference_buffer) = match &reference_anim {
        Anim::V20 { groups, buffer, .. } | Anim::V21 { groups, buffer, .. } => (groups, buffer),
        Anim::V12 { .. } => {
            return Err(anyhow::format_err!("v12 reference anim not supported!"));
        }
    };

    let (modified_groups, modified_buffer) = match &modified_anim {
        Anim::V20 { groups, buffer, .. } | Anim::V21 { groups, buffer, .. } => (groups, buffer),
        Anim::V12 { .. } => {
            return Err(anyhow::format_err!("v12 modified anim not supported!"));
        }
    };

    let reference_transform_group = reference_groups
        .elements
        .iter()
        .find(|group_entry| group_entry.group_type == GroupType::Transform);

    let reference_transform_nodes_data: Vec<AnimTransformNodeData> = match reference_transform_group
    {
        Some(group) => group
            .nodes
            .elements
            .iter()
            .map(|node| AnimTransformNodeData::from(node, reference_buffer))
            .collect(),
        None => Vec::new(),
    };

    let modified_transform_group = modified_groups
        .elements
        .iter()
        .find(|group_entry| group_entry.group_type == GroupType::Transform);

    // Basically the transform data of added bones in the new anim ONLY.
    let modified_exclusive_transform_nodes_data: Vec<AnimTransformNodeData> =
        match modified_transform_group {
            Some(group) => group
                .nodes
                .elements
                .iter()
                .filter(|mod_node| {
                    !reference_transform_nodes_data
                        .iter()
                        .any(|ref_node| mod_node.name.to_string_lossy() == ref_node.name)
                })
                .map(|mod_node| AnimTransformNodeData::from(mod_node, modified_buffer))
                .collect(),
            None => Vec::new(),
        };

    let spliced_transform_nodes_data: Vec<AnimTransformNodeData> = reference_transform_nodes_data
        .iter()
        .cloned()
        .chain(modified_exclusive_transform_nodes_data.iter().cloned())
        .sorted_by_key(|x| x.name.to_lowercase())
        .collect::<Vec<_>>();

    let mut current_offset: u64 = 0;
    let mut new_buffer = SsbhByteBuffer::new();
    let mut new_groups: SsbhArray<ssbh_lib::formats::anim::Group> = SsbhArray::new();

    if !spliced_transform_nodes_data.is_empty() {
        let mut new_transform_group = ssbh_lib::formats::anim::Group {
            group_type: ssbh_lib::formats::anim::GroupType::Transform,
            nodes: SsbhArray::new(),
        };
        for node_data in &spliced_transform_nodes_data {
            let new_node = ssbh_lib::formats::anim::Node {
                name: node_data.name.clone().into(),
                tracks: SsbhArray::from_vec(vec![TrackV2 {
                    data_offset: current_offset as u32,
                    ..node_data.track.clone()
                }]),
            };

            new_buffer.elements.extend_from_slice(&node_data.buffer);
            current_offset += node_data.buffer.len() as u64;
            new_transform_group.nodes.elements.push(new_node);
        }
        new_groups.elements.push(new_transform_group);
    }

    let spliced_vis_group_and_buf = get_anim_group_and_buffer_with_fallback(
        modified_groups,
        modified_buffer,
        reference_groups,
        reference_buffer,
        GroupType::Visibility,
    );

    let spliced_mat_group_and_buf = get_anim_group_and_buffer_with_fallback(
        modified_groups,
        modified_buffer,
        reference_groups,
        reference_buffer,
        GroupType::Material,
    );

    for spliced_group in vec![spliced_vis_group_and_buf, spliced_mat_group_and_buf]
        .into_iter()
        .flatten()
    {
        let mut new_group = Group {
            group_type: spliced_group.group.group_type,
            nodes: SsbhArray::new(),
        };
        for old_node in &spliced_group.group.nodes.elements {
            let mut new_node = Node {
                name: old_node.name.clone(),
                tracks: SsbhArray::new(),
            };
            for old_track in &old_node.tracks.elements {
                let new_track = TrackV2 {
                    data_offset: current_offset as u32,
                    ..old_track.clone()
                };
                let start_index = old_track.data_offset as usize;
                let end_index = (old_track.data_offset as u64 + old_track.data_size) as usize;
                let old_buffer = spliced_group.buffer;
                let slice = &old_buffer.elements[start_index..end_index];
                new_buffer.elements.extend_from_slice(slice);
                current_offset += slice.len() as u64;
                new_node.tracks.elements.push(new_track);
            }
            new_group.nodes.elements.push(new_node);
        }
        new_groups.elements.push(new_group);
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
