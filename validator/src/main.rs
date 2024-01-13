use anyhow::Result;
use clap::Parser;
use ssbh_data::prelude::*;
use std::collections::HashMap;
use std::iter::zip;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use ssbh_data::anim_data::{GroupType, NodeData, TrackValues};

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

fn get_group_by_type(
    anim_data: &AnimData,
    group_type: ssbh_data::anim_data::GroupType,
) -> Option<&ssbh_data::anim_data::GroupData> {
    for group in &anim_data.groups {
        if group.group_type == group_type {
            return Some(&group);
        }
    }
    None
}

fn validate_anim(reference_anim_path: &PathBuf, modified_anim_path: &PathBuf) -> SafetyRating {
    let reference_anim = match ssbh_data::anim_data::AnimData::from_file(&reference_anim_path) {
        Ok(anim) => anim,
        Err(e) => {
            return SafetyRating::Warning(format!(
                "Reference anim could not be opened by ssbh_data, error=`{}`",
                e
            ));
        }
    };

    let modified_anim = match ssbh_data::anim_data::AnimData::from_file(&modified_anim_path) {
        Ok(anim) => anim,
        Err(e) => {
            return SafetyRating::Warning(format!(
                "Modified anim could not be opened by ssbh_data, error=`{}`",
                e
            ));
        }
    };

    if reference_anim.final_frame_index != modified_anim.final_frame_index {
        return SafetyRating::Unsafe(
            format!(
                "The modifed anim has a final_frame_index of `{}`, while the matching vanilla anim has a final_frame_index of `{}`",
                modified_anim.final_frame_index,
                reference_anim.final_frame_index
            )
        );
    }

    let (ref_trans_group, mod_trans_group) = match (get_group_by_type(&reference_anim, GroupType::Transform), get_group_by_type(&modified_anim, GroupType::Transform)){
        (Some(ref_group), Some(mod_group)) => {(ref_group, mod_group)},
        (Some(_ref_group), None) => {return SafetyRating::Unsafe("The reference anim has a transform group, but the modified group has no transform group!".to_string())},
        (None, Some(_mod_group)) => {return SafetyRating::Warning("The modified anim has transform data, but the vanilla anim had none! As long as you're 100% sure you didn't mess with any vanilla hitbox/hurtbox bones, this can still be ok.".to_string())},
        (None, None) => {return SafetyRating::Safe}
    };

    let mod_nodes_by_name: HashMap<String, &NodeData> = mod_trans_group.nodes.iter().map(|x| (x.name.clone(), x)).collect();

    // For the Transform group, each Node corresponds to a bone.
    // Each bone Node will only have one Track, which is it's transform values.
    for reference_node in &ref_trans_group.nodes {
        let reference_values = match reference_node.tracks.get(0) {
            None => {
                println!("The reference anim {:?} has a Node for bone `{}` with no transform Track at all! Skipping this bone..", reference_anim_path.file_name().unwrap_or_default(), reference_node.name);
                continue;
             }
             Some(track) => {
                match &track.values{
                    TrackValues::Transform(values) => {values},
                    _ => {
                        /* Some vanilla anims like
                        fighter/kirby/motion/jackbody/c00/jackd00specialairnrandomend.nuanmb
                        are poorly formatted like this.
                        */
                        println!("The reference anim `{:?}` is poorly formatted and has vis or mat data in the transform data. Skipping that track...", reference_anim_path.file_name().unwrap_or_default());
                        continue;
                    }
                }
             },
        };

        let modified_node = match mod_nodes_by_name.get(&reference_node.name){
            Some(node) => {node},
            None => {return SafetyRating::Unsafe(format!("Modified anim missing transform node `{}`", reference_node.name))}
        };

        let modified_values = match modified_node.tracks.get(0) {
            None => {return SafetyRating::Unsafe(format!("The modified anim is missing the Transform Track for Node `{}`", modified_node.name))},
            Some(track) => match &track.values{
                TrackValues::Transform(values) => {values},
                _ => {return SafetyRating::Unsafe(format!("The modified anim is poorly formatted and has vis or mat data instead of transform data for Node `{}`", modified_node.name))}
            }
        };

        if reference_values.len() != modified_values.len(){
            return SafetyRating::Unsafe(format!(
                "The Node `{}` has different amount of values in the vanilla vs the modified! Vanilla=`{}`, Modified=`{}`", 
                modified_node.name,
                reference_values.len(),
                modified_values.len()))
        }

        for (index, (reference_value, modified_value)) in zip(reference_values.iter(), modified_values.iter()).enumerate(){
            if reference_value != modified_value {
                return SafetyRating::Unsafe(
                    format!(
                        "The Node `{}` at frame `{}` has differing values! Vanilla=`{:?}`, Modified=`{:?}`",
                        modified_node.name,
                        index,
                        reference_value,
                        modified_value,
                    )
                );
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

        match validate_anim(&matching_vanilla_anim_path, &modified_anim_path) {
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

    let modified_dir = args
        .modified_folder
        .expect("Modified Folder not provided!");

    println!("Now validating, please wait...");
    let result = validate_dirs(&reference_dir, &modified_dir);
    println!("Done! elapsed time = {:?}!", start_time.elapsed());
    result
}
