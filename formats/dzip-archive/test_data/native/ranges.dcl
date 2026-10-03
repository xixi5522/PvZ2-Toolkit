archive ranges.dz
archive ranges-1.dz
archive ranges-2.dz
basedir corpus
align 4096
file common\base.bin 0 dz
file common\variant-1.bin 1 dz to 25%
file common\variant-1.bin 1 dz from 25% to 75%
file common\variant-1.bin 1 dz from 75%
file local\periodic.bin 2 dz from 123 to 12001

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
