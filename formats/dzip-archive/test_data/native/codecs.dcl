archive codecs.dz
archive codecs-1.dz
basedir corpus
file common\base.bin 0 dz
file common\variant-1.bin 1 dz jpeg
file common\variant-2.bin 0 dz mp3
file common\variant-3.bin 1 dz jpeg mp3 random-access
file local\random.bin 1 zlib random-access
file local\text.txt 1 bzip
file local\periodic.bin 1 lzma
file local\runs.bin 0 copy random-access
file local\zero.bin 0 zero

options dz
isnotdefault 1
max_mem_usage -1
use_combuf 1
preprocess 1
trim_reference_factor 20
WinSize 16
Flags 1
OffsetTableSize 8
OffsetTables 3
OffsetContexts 3
RefLengthTableSize 7
RefLengthTables 1
RefOffsetTableSize 7
RefOffsetTables 3
BigMinMatch 15
