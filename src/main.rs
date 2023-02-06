use clap::Parser;
use ssbh_lib::{prelude::*, SsbhArray, formats::anim::{GroupType}, SsbhByteBuffer};
use anyhow::{Context, Result};
use std::time::Instant;

#[derive(Parser)]
#[command(author, version, about, long_about=None)]
struct Args {
    #[arg(short, long)]
    reference_anim: std::path::PathBuf,
    #[arg(short,long)]
    modified_anim: std::path::PathBuf,
    #[arg(short, long)]
    output_path: std::path::PathBuf,
}
fn main() -> Result<()> {
    let start_time = Instant::now();
    let args = Args::parse();
    let reference_anim = ssbh_lib::formats::anim::Anim::from_file(&args.reference_anim)
        .with_context(|| format!("coult not read reference anim `{}`", &args.reference_anim.display()))?;
    let modified_anim = ssbh_lib::formats::anim::Anim::from_file(&args.modified_anim)
        .with_context(|| format!("coult not read modified anim `{}`", &args.modified_anim.display()))?;

    // Validate Anim Versions
    if let Anim::V12{..} = &reference_anim {
        return Err(anyhow::format_err!("v12 reference anim not supported!"))
    }
    if let Anim::V12{..} = &modified_anim {
        return Err(anyhow::format_err!("v12 modified anim not supported!"))
    }

    // Gather reference data
    let mut reference_node_name_to_buffer = std::collections::HashMap::new();
    let mut reference_node_name_to_track = std::collections::HashMap::new();
    let mut reference_node_names = Vec::new();
    if let Anim::V21 {groups, buffer , .. } | Anim::V20 {groups, buffer , .. } = &reference_anim {
        for group in &groups.elements{
            if group.group_type != GroupType::Transform{
                continue
            }
            for node in &group.nodes.elements{
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
    if let Anim::V21 {groups, .. } | Anim::V20 {groups, .. } = &modified_anim{
        for group in &groups.elements{
            modified_group_types.push(group.group_type);
        }    
    }

    // Now format the new buffer
    let mut current_offset: u64 = 0;
    let mut new_buffer = SsbhByteBuffer::new();
    let mut new_groups: SsbhArray<ssbh_lib::formats::anim::Group> = SsbhArray::new();
    // First just go through the vanilla anim's bone data and just copy all those buffers as-is.
    // Then we can just grab any new bone data and the vis/mat tracks from the modified anim
    let mut new_transform_group = ssbh_lib::formats::anim::Group{
        group_type: ssbh_lib::formats::anim::GroupType::Transform,
        nodes: SsbhArray::new()
    };
    if let Anim::V20{groups, ..} | Anim::V21{groups, ..} = &reference_anim{
        for reference_group in &groups.elements{
            if reference_group.group_type != GroupType::Transform{
                continue;
            }
            for reference_node in &reference_group.nodes.elements{
                let mut new_node = ssbh_lib::formats::anim::Node{
                    name: reference_node.name.clone(),
                    tracks: SsbhArray::new()
                };
                for reference_track in &reference_node.tracks.elements{
                    let new_track = ssbh_lib::formats::anim::TrackV2{
                        name: reference_track.name.clone(),
                        flags: reference_track.flags,
                        frame_count: reference_track.frame_count,
                        transform_flags: reference_track.transform_flags,
                        data_offset: current_offset as u32,
                        data_size: reference_track.data_size
                    };
                    let reference_buffer = reference_node_name_to_buffer.get(reference_node.name.to_str().unwrap()).unwrap();
                    new_buffer.elements.extend_from_slice(&reference_buffer);
                    current_offset += reference_track.data_size;
                    new_node.tracks.elements.push(new_track);
                }
                new_transform_group.nodes.elements.push(new_node);
            }  
        }
    }

    // At this point, only one transform group has been made.
    // Gather new bone data to finish the new transform group
    if let Anim::V20{groups, buffer, ..} | Anim::V21{groups, buffer, ..} = &modified_anim{
        for modified_group in &groups.elements{
            if modified_group.group_type != GroupType::Transform{
                continue;
            }
            for modified_node in &modified_group.nodes.elements{
                if reference_node_names.contains(&modified_node.name){
                    continue;
                } 
                let mut new_node = ssbh_lib::formats::anim::Node{
                    name: modified_node.name.clone(),
                    tracks: SsbhArray::new()
                };
                for modified_track in &modified_node.tracks.elements{
                    let new_track = ssbh_lib::formats::anim::TrackV2{
                        name: modified_track.name.clone(),
                        flags: modified_track.flags,
                        frame_count: modified_track.frame_count,
                        transform_flags: modified_track.transform_flags,
                        data_offset: current_offset as u32,
                        data_size: modified_track.data_size
                    };
                    let start_index = modified_track.data_offset as usize;
                    let end_index = (modified_track.data_offset as u64 + modified_track.data_size) as usize;
                    let modified_buffer = &buffer.elements[start_index..end_index];
                    new_buffer.elements.extend_from_slice(&modified_buffer);
                    current_offset += modified_track.data_size;
                    new_node.tracks.elements.push(new_track);
                }
                new_transform_group.nodes.elements.push(new_node);
            }
        }
    }
    new_groups.elements.push(new_transform_group);

    // Now we need to grab the mat/vis groups from the modified anim
    if let Anim::V20{groups, buffer, ..} | Anim::V21{groups, buffer, ..} = &modified_anim{
        for modified_group in &groups.elements{
            if modified_group.group_type == GroupType::Transform{
                continue
            }
            let mut new_group = ssbh_lib::formats::anim::Group{
                group_type: modified_group.group_type,
                nodes: SsbhArray::new()
            };
            for modified_node in &modified_group.nodes.elements{
                let mut new_node = ssbh_lib::formats::anim::Node{
                    name: modified_node.name.clone(),
                    tracks: SsbhArray::new()
                };
                for modified_track in &modified_node.tracks.elements{
                    let new_track = ssbh_lib::formats::anim::TrackV2{
                        name: modified_track.name.clone(),
                        flags: modified_track.flags,
                        frame_count: modified_track.frame_count,
                        transform_flags: modified_track.transform_flags,
                        data_offset: current_offset as u32,
                        data_size: modified_track.data_size
                    };
                    let start_index = modified_track.data_offset as usize;
                    let end_index = (modified_track.data_offset as u64 + modified_track.data_size) as usize;
                    let modified_buffer = &buffer.elements[start_index..end_index];
                    new_buffer.elements.extend_from_slice(&modified_buffer);
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
    if let Anim::V20{groups, ..} | Anim::V21{groups, ..} = &reference_anim{
        for reference_group in &groups.elements{
            if reference_group.group_type == GroupType::Transform{
                continue;
            }
            if modified_group_types.contains(&reference_group.group_type){
                continue;
            }
            let mut new_group = ssbh_lib::formats::anim::Group{
                group_type: reference_group.group_type,
                nodes: SsbhArray::new()
            };
            for reference_node in &reference_group.nodes.elements{
                let mut new_node = ssbh_lib::formats::anim::Node{
                    name: reference_node.name.clone(),
                    tracks: SsbhArray::new()
                };
                for reference_track in &reference_node.tracks.elements{
                    let new_track = ssbh_lib::formats::anim::TrackV2{
                        name: reference_track.name.clone(),
                        flags: reference_track.flags,
                        frame_count: reference_track.frame_count,
                        transform_flags: reference_track.transform_flags,
                        data_offset: current_offset as u32,
                        data_size: reference_track.data_size
                    };
                    let reference_buffer = reference_node_name_to_buffer.get(reference_node.name.to_str().unwrap()).unwrap();
                    new_buffer.elements.extend_from_slice(&reference_buffer);
                    current_offset += reference_track.data_size;
                    new_node.tracks.elements.push(new_track);
                }
                new_group.nodes.elements.push(new_node);
            }
            new_groups.elements.push(new_group);
        }
    }

    /* 
    if let Anim::V20{groups, buffer, ..} | Anim::V21{groups, buffer, ..} = &modified_anim{
        for modified_group in &groups.elements{
            let mut new_group = ssbh_lib::formats::anim::Group{
                group_type: modified_group.group_type,
                nodes: SsbhArray::new()
            };
            for modified_node in &modified_group.nodes.elements{
                let mut new_node = ssbh_lib::formats::anim::Node{
                    name: modified_node.name.clone(),
                    tracks: SsbhArray::new()
                };
                for modified_track in &modified_node.tracks.elements{
                    let reference_track = reference_node_name_to_track.get(modified_node.name.to_str().unwrap());
                    let new_track = match reference_track{
                        Some(reference_track) => {
                            ssbh_lib::formats::anim::TrackV2{
                                name: reference_track.name.clone(),
                                flags: reference_track.flags,
                                frame_count: reference_track.frame_count,
                                transform_flags: reference_track.transform_flags,
                                data_offset: current_offset as u32,
                                data_size: reference_track.data_size
                            }
                        },
                        None => {
                            ssbh_lib::formats::anim::TrackV2{
                                name: modified_track.name.clone(),
                                flags: modified_track.flags,
                                frame_count: modified_track.frame_count,
                                transform_flags: modified_track.transform_flags,
                                data_offset: current_offset as u32,
                                data_size: modified_track.data_size
                            }
                        }
                    };
                    if let Some(reference_track) = reference_track {
                        let reference_buffer = reference_node_name_to_buffer.get(modified_node.name.to_str().unwrap()).unwrap();
                        new_buffer.elements.extend_from_slice(&reference_buffer);
                        current_offset += reference_track.data_size;
                    } else {
                        let start_index = modified_track.data_offset as usize;
                        let end_index = (modified_track.data_offset as u64 + modified_track.data_size) as usize;
                        let modified_buffer = &buffer.elements[start_index..end_index];
                        new_buffer.elements.extend_from_slice(&modified_buffer);
                        current_offset += modified_track.data_size;
                    };
                    new_node.tracks.elements.push(new_track);
                }
                new_group.nodes.elements.push(new_node);
            }
            new_groups.elements.push(new_group);
        }
    }
    */
    let new_anim = match reference_anim {
        Anim::V20 {final_frame_index, unk1, unk2, name, .. } => {
            Ok(Anim::V20 { 
                final_frame_index: final_frame_index, 
                unk1: unk1, 
                unk2: unk2, 
                name: name.clone(), 
                groups: new_groups, 
                buffer: new_buffer })
        }
        Anim::V21 {final_frame_index, unk1, unk2, name, unk_data, .. }  => {
            Ok(Anim::V21 { 
                final_frame_index: final_frame_index,
                unk1: unk1,
                unk2: unk2,
                name: name.clone(),
                groups: new_groups,
                buffer: new_buffer,
                unk_data: unk_data })
        }
        _ => Err(anyhow::format_err!("Got an unsupported reference anim but this code should have never been reached "))
    };

    new_anim?.write_to_file(&args.output_path)
        .with_context(|| format!("could not output the new anim to the output path `{}`", &args.output_path.display()))?;

    println!("Done! elapsed time = {:?}!", start_time.elapsed());
    Ok(())
}
