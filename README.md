# wifisafe_anim_splicer
A CLI for splicing together a 100% wifi-safe SSBU anim from a reference anim and modified anim in Rust.

## "wifi safe"
* "wifi-safe" is the community term for a modified file which will not cause issues when connecting to other online peers without the file.
* A modified animation is "wifi safe" as long as every bone with a hitbox attached has it's animation data unmodified.
## 99.9999% "wifi-safe" vs 100% "wifi-safe"
The SSBU `.nuanmb` file format uses lossy compression, as such importing a `.nuanmb` into a program and exporting it even without modifications will result in an animation that is 99.9999% close to the original, but not quite 100% and while such an animation is practicaly "wifi safe" it is not guaranteed to be and users will be hesitant in trusting it.
## Common Usecases
1. Modifying just the Visibility or Material data of a `.nuanmb` while keeping the Transfom data 100% intact.
2. Adding new bones to an existing animation while keeping the existing bones unmodified.

   ( e.g. an animation that addresses the lack of translation for bones in mods made using the 'Exo Skel' method.)

## Usage   
* Grab the .exe from the 'Releases' page
* Run it from the command line (don't double click the .exe)
* Use -h or --help to see the parameters needed to run it properly.
#### Example usage 
* `wifisafe_anim_splicer.exe -r vanilla_palu_ftillt.nuanmb -m modified_palu_ftilt.nuanmb -o output.nuanmb`
#### Example usage (Batch Mode)
* If the folders are layed out like this-
* ![image](https://user-images.githubusercontent.com/77519735/230791951-8129a147-5d58-4d76-871f-c7d55412156d.png)
* Then the command looks like this
* `wifisafe_anim_splicer.exe --reference_folder "vanilla_anims" --modified_folder "modded_anims" --output_folder "output_folder"`
   
* Verify the resulting `.nuanmb` is wifi-safe by grabbing [ssbh_data_json](https://github.com/ultimate-research/ssbh_lib/releases) and comparing the new .nuanmb's JSON vs the reference's JSON.

* If its wifi-safe, there should be no difference in the Transform data for the vanilla bones.
## Supported Anims
* Only supports V20 and V21 `.nuanmb` files.
* Not tested with `.nuanmb` files from any game besides SSBU.

# validator
* Does the "validate the resulting anim with ssbh_data" step for a whole folder of anims for you
* If you are submitting a mod with hundreds of exported anims, this is a good way to check without manually opening all 300+ anims.
## Usage
* Grab the .exe from the 'Releases' page
* Run it from the command line (don't double click the .exe)
* Use -h or --help to see the parameters needed to run it properly.
### Example 
#### Example Use
* If the folders are layed out like this-
* ![image](https://user-images.githubusercontent.com/77519735/235803544-570aec59-2399-4ed9-854b-e45be4915a10.png)
* Then the command looks like this
* `validator.exe -r vanilla_anims -m modded_anims`

#### Example Output
```
UNSAFE: Anim="a02dash.nuanmb", reason=`The modified anim has different values than the vanilla for bone `ArmR`!`
UNSAFE: Anim="f01damageflyrollend.nuanmb", reason=`The modifed anim has a final_frame_index of `48`, while the matching vanilla anim has a final_frame_index of `25``
SKIPPED: Skipping j02lose.nuanmb, since it's name starts with `j02` and is a victory screen animation.
SKIPPED: Skipping j02win1.nuanmb, since it's name starts with `j02` and is a victory screen animation.
SKIPPED: Skipping j02win1wait.nuanmb, since it's name starts with `j02` and is a victory screen animation.
SKIPPED: Skipping j02win2.nuanmb, since it's name starts with `j02` and is a victory screen animation.
SKIPPED: Skipping j02win2wait.nuanmb, since it's name starts with `j02` and is a victory screen animation.
SKIPPED: Skipping j02win3.nuanmb, since it's name starts with `j02` and is a victory screen animation.
SKIPPED: Skipping j02win3wait.nuanmb, since it's name starts with `j02` and is a victory screen animation.
Total Modified Anims: 310
Unsafe Count: 2
Warning Count: 0
Skip Count: 7
```
* In this case, i now know the anims `a02dash.nuanmb` and `f01damageflyrollend.nuanmb` must be spliced and checked again.
* Most of the time, this won't be an issue.



