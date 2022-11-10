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
* Example usage 

   `wifisafe_anim_splicer.exe -r vanilla_palu_ftillt.nuanmb -m modified_palu_ftilt.nuanmb -o output.nuanmb`
* Verify the resulting `.nuanmb` is wifi-safe by grabbing [ssbh_data_json](https://github.com/ultimate-research/ssbh_lib/releases) and comparing the new .nuanmb's JSON vs the reference's JSON.

   If its wifi-safe, there should be no difference in the Transform data for the vanilla bones.
## Supported Anims
* Only supports V20 and V21 `.nuanmb` files.
* Not tested with `.nuanmb` files from any game besides SSBU.
