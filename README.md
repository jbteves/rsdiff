# DON'T USE
Don't use this.
It will be supplanted by a new tool in the future.
Please consider using AFNI's 3dDiff instead.

# What is this?
This is a highly experimental, fragile, and temperamental tool.
Use at your own risk.
`rsdiff` will compute a diff, including voxelwise diffs on niftis.
It is very niche, but based on my calculations is about as fast as using
system calls and built-ins from image calculators, without creating a mess
of temporary files.
Usage is available via `rsdiff -h`, but is basically the same as using the
common tool `diff`:
```
rsdiff left right
... output of the diff...
```
with a few notable exceptions: it does do niftis (yay!) but it cannot diff
anything else except byte-wise (boo!).
One amusing thing to note is that byte-wise diffing is not accurate for
niftis, but actually gets you a very good ballpark in a fraction of the
time in the case of gzipped-niftis.
If your niftis are not gzipped, this will rip along at about 1s/GB.
If your niftis are gzipped, this will slow to tortoise-like speeds; it
quite literally can cost a factor of ten in speed.

# Installation
This isn't available as a crate yet because it's a prototype.
You'll have to [install Rust](https://www.rust-lang.org/tools/install).
From there, you can clone this repository with `git`.
Then run the following
```bash
cd /path/to/repo
cargo build --release # (you will get warnings because this is a prototype)
```
From here, you have a few options.
You can either alias the executable, which is located in
`./target/rsdiff`:
```
alias rsdiff=/path/to/repo/target/rsdiff
```
or you can link the executable to some position along your path.
