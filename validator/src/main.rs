use anyhow::Result;
use clap::Parser;
use ssbh_data::prelude::*;
use ssbh_lib::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
struct Args {
    #[arg(short = 'r', long = "reference_folder")]
    reference_folder: Option<PathBuf>,
    #[arg(short = 'm', long = "modified_folder")]
    modified_folder: Option<PathBuf>,
}

enum SafetyRating {
    Safe,
    Unsafe(String),
    Warning(String),
}
fn validate_anims(reference_anim_path: &PathBuf, modified_anim_path: &PathBuf) -> SafetyRating {
    let modified_anim = match ssbh_lib::formats::anim::Anim::from_file(&modified_anim_path) {
        Ok(anim) => anim,
        Err(e) => {
            return SafetyRating::Warning(format!(
                "Can't validate modified file, it could not be opened by ssbh_lib, error=`{}`",
                e
            ));
        }
    };

    let reference_anim = match ssbh_lib::formats::anim::Anim::from_file(&reference_anim_path) {
        Ok(anim) => anim,
        Err(e) => {
            return SafetyRating::Warning(
                format!("Can't validate modified file, it's matching reference anim could not be opened by ssbh_lib, error=`{}`", e)
            );
        }
    };

    if let Anim::V12 { .. } = &modified_anim {
        return SafetyRating::Warning(
            format!("Can't validate modified file, its version 1.2 and could not have been made with the wifi_safe_anim_splicer!")
        );
    }

    if let Anim::V12 { .. } = &reference_anim {
        return SafetyRating::Unsafe(format!(
            "The modifed anim has a matching vanilla animation that is version V.1.2!"
        ));
    }

    let modified_final_frame_index = match modified_anim {
        Anim::V21 {
            final_frame_index, ..
        }
        | Anim::V20 {
            final_frame_index, ..
        } => final_frame_index,
        _ => {
            panic!("Unreachable code reached")
        }
    };

    let reference_final_frame_index = match reference_anim {
        Anim::V21 {
            final_frame_index, ..
        }
        | Anim::V20 {
            final_frame_index, ..
        } => final_frame_index,
        _ => {
            panic!("Unreachable code reached")
        }
    };

    if modified_final_frame_index != reference_final_frame_index {
        return SafetyRating::Unsafe(
            format!(
                "The modifed anim has a final_frame_index of `{modified_final_frame_index}`, while the matching vanilla anim has a final_frame_index of `{reference_final_frame_index}`"
            )
        );
    }

    let reference_data = match ssbh_data::anim_data::AnimData::from_file(reference_anim_path) {
        Err(e) => {
            return SafetyRating::Warning(
                format!("Can't validate modified file, it's matching reference anim could not be opened by ssbh_data, error=`{}`", e)
            );
        }
        Ok(data) => data,
    };

    let modified_data = match ssbh_data::anim_data::AnimData::from_file(modified_anim_path) {
        Err(e) => {
            return SafetyRating::Warning(format!(
                "Can't validate modified file, it couldn't be read by ssbh_data, error=`{}`",
                e
            ));
        }
        Ok(data) => data,
    };

    use std::collections::HashMap;
    let mut modified_bone_name_to_track = HashMap::new();
    for group in modified_data.groups {
        if group.group_type != ssbh_data::anim_data::GroupType::Transform {
            continue;
        }
        for node in group.nodes {
            modified_bone_name_to_track.insert(node.name.clone(), node.tracks[0].clone());
        }
    }

    for group in reference_data.groups {
        if group.group_type != ssbh_data::anim_data::GroupType::Transform {
            continue;
        }
        for reference_node in group.nodes {
            let modified_track = match modified_bone_name_to_track.get(&reference_node.name) {
                Some(track) => track,
                None => {
                    return SafetyRating::Unsafe(format!(
                        "The modifed anim is missing the transform track for bone {}!",
                        reference_node.name
                    ));
                }
            };
            let reference_values = match &reference_node.tracks[0].values {
                ssbh_data::anim_data::TrackValues::Transform(values) => values,
                _ => {
                    panic!()
                }
            };
            let modified_values = match &modified_track.values {
                ssbh_data::anim_data::TrackValues::Transform(values) => values,
                _ => {
                    panic!()
                }
            };

            for (index, reference_value) in reference_values.iter().enumerate() {
                let modified_value =  match modified_values.get(index){
                    Some(value) => {value}
                    None => return SafetyRating::Unsafe(
                        format!(
                            "The modified anim has a transform track for bone {}, but its length is shorter than the reference!", reference_node.name
                        )
                    )
                };

                if reference_value != modified_value {
                    return SafetyRating::Unsafe(format!(
                        "The modified anim has different values than the vanilla for bone `{}`!",
                        reference_node.name
                    ));
                }
            }
        }
    }

    SafetyRating::Safe
}

fn validate_dirs(reference_dir: &PathBuf, modified_dir: &PathBuf) -> Result<()> {
    if reference_dir == modified_dir {
        return Err(anyhow::format_err!(
            "Specified 'Reference' and 'Modified' folders are the same folders!"
        ));
    }
    let reference_anim_paths = fs::read_dir(reference_dir)
        .unwrap()
        .filter_map(|dir_entry| dir_entry.ok())
        .map(|dir_entry| dir_entry.path())
        .filter(|path| path.extension().unwrap().eq("nuanmb"))
        .collect::<Vec<_>>();

    let modified_anim_paths = fs::read_dir(modified_dir)
        .unwrap()
        .filter_map(|dir_entry| dir_entry.ok())
        .map(|dir_entry| dir_entry.path())
        .filter(|path| path.extension().unwrap().eq("nuanmb"))
        .collect::<Vec<_>>();

    let mut warning_count = 0;
    let mut unsafe_count = 0;
    let mut skip_count = 0;

    for modified_anim_path in &modified_anim_paths {
        let modified_anim_file_name = modified_anim_path
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        if modified_anim_file_name.starts_with("j02") {
            println!("SKIPPED: Skipping {modified_anim_file_name}, since it's name starts with `j02` and is a victory screen animation.");
            skip_count += 1;
            continue;
        }

        let matching_vanilla_anim_path: PathBuf = match reference_anim_paths
            .iter()
            .find(|&p| p.file_name() == modified_anim_path.file_name())
        {
            Some(path) => path.clone(),
            None => {
                println!(
                    "WARNING: Can't validate modified file {modified_anim_path:?}, no vanilla anim was found!"
                );
                warning_count += 1;
                continue;
            }
        };

        match validate_anims(&matching_vanilla_anim_path, &modified_anim_path) {
            SafetyRating::Safe => {}
            SafetyRating::Unsafe(msg) => {
                println!(
                    "UNSAFE: Anim={:?}, reason=`{}`",
                    modified_anim_path.file_name().unwrap_or_default(),
                    msg
                );
                unsafe_count += 1;
            }
            SafetyRating::Warning(msg) => {
                println!(
                    "WARNING: Anim={:?}, reason=`{}`",
                    modified_anim_path.file_name().unwrap_or_default(),
                    msg
                );
                warning_count += 1;
            }
        };
    }

    println!("Total Modified Anims: {}", &modified_anim_paths.len());
    println!("Unsafe Count: {}", unsafe_count);
    println!("Warning Count: {}", warning_count);
    println!("Skip Count: {}", skip_count);
    Ok(())
}

fn main() -> Result<()> {
    let start_time = Instant::now();

    let args = Args::parse();

    let reference_dir = args
        .reference_folder
        .expect("Reference Folder not provided!");

    let modified_dir = args.modified_folder.expect("Modified Folder not provided!");

    println!("Now validating, please wait...");
    let result = validate_dirs(&reference_dir, &modified_dir);
    println!("Done! elapsed time = {:?}!", start_time.elapsed());
    result
}
