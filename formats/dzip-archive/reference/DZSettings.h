/*
 * (C) 2001-2012 Marmalade. All Rights Reserved.
 *
 * This document is protected by copyright, and contains information
 * proprietary to Marmalade.
 *
 * This file consists of source code released by Marmalade under
 * the terms of the accompanying End User License Agreement (EULA).
 * Please do not use this program/source code before you have read the
 * EULA and have agreed to be bound by its terms.
 */
//DZSettings.h - definition of settings structure for range encoding/decoding file format

#define DZ_VERSION 0

/*Version 0 file format is:
    ArchiveSettings
    User File List (ArchiveSettings.NumUserFiles list of null-terminated files)
    DirectoryList (ArchiveSettings.NumDirectories list of null-terminated files)
    User-File to Chunk-And-Directory list (see below)

    ChunkSettings
    Chunk List (ChunkSettings.NumChunks list of Chunk structures)
    File List (ChunkSettings.NumArchiveFiles -1 list of null-terminated files)

    Various global decoder settings for all the decoders needed to decompress this archive, in the
    order of the occurance of the decoder in the Chunk flags, below. Each is decoder-specific and can be none.

    File data
*/


/*
    User-File to Chunk-And-Directory list: This is a list of 16-bit unsigned shorts.
    For each user file, the directory ID of the directory the user file belongs to is stored.
    Then follows a list of chunks ID's that make up the user file, in the order of their
    occurance in the user file. The list is terminated by 0xffff.
    Large Chunks that span one or more files will have their Chunk ID's listed for more than
    one file. The files referencing the same Chunk IDs must all be adjacent in the file list.
    The decompression system works out the file to chunk mapping using the chunk/file length
    etc.
*/

#define MAX_OFFSET_CONTEXTS  8      //maximum number of different offset contexts allowed
#define MAX_CHUNKS           65535  //maximum number of chunks in the archive
#define MAX_WINSIZE          30     //maximum possible window size
#define MAX_DECODERS         16     //maximum number of decoders that can be used

#ifndef __GNUC__
#pragma pack (push)         //Don't allow any structure padding
#pragma pack (1)
#endif

#include "s3eTypes.h"

IW_UNALIGNED struct ArchiveSettings
{
    Uint   Header;          //Identification 'DTRZ'
    Ushort NumUserFiles;    //number of original user-files stored in this archive
    Ushort NumDirectories;  //number of stored directories
    uc version;             //version ID of this settings structure
} IW_UNALIGNED_END;
#define ARCHIVESETTINGS_SIZE 9

IW_UNALIGNED struct ChunkSettings
{
    Ushort NumArchiveFiles; //number of files used to store this archive
    Ushort NumChunks;       //number of chunks they're divided up into
} IW_UNALIGNED_END;
#define CHUNKSETTINGS_SIZE 4

IW_UNALIGNED struct Chunk
{
    Uint offset;            //The location of the chunk in its file
    Uint compressed_length;   //Lengh of compressed chunk (mainly for use of combufs)
    Uint decompressed_length; //Lengh of original data
    Ushort flags;           //Chunk flags
    Ushort File;            //Which file this chunk's compressed data lives in
 } IW_UNALIGNED_END;
#define CHUNK_SIZE 16

//Chunk flags:
#define CHUNK_COMBUF            0x1     //Set to indicate a combuf chunk. Combuf chunks are all appended to each other
#define CHUNK_DZ                0x4     //Set to indicate a dzip chunk, for use with range decoder
#define CHUNK_ZLIB              0x8     //Set to indicate a zlib chunk
#define CHUNK_BZIP              0x10    //Set to indicate a bzip2 chunk
#define CHUNK_MP3               0x20    //Set to indicate a mp3 chunk
#define CHUNK_JPEG              0x40    //Set to indicate a JPEG chunk
#define CHUNK_ZERO              0x80    //Set to indicate a zerod-out chunk
#define CHUNK_COPYCOMP          0x100   //Set to indicate a copy-coded (ie no compression) chunk
#define CHUNK_LZMA              0x200   //Set to indicate a lzma encoded chunk
#define CHUNK_RANDOMACCESS      0x400   //Set to indicate whole chunk should be buffered by the decoder for random access



IW_UNALIGNED struct RangeSettings
{
    uc WinSize;             //log2(LZ-77 window size)
    uc Flags;               //Settings for rangedecoding
    uc OffsetTableSize;     //log2(LZ-77 match offset frequency table size)
    uc OffsetTables;        //number of LZ-77 offset frequency tables
    uc OffsetContexts;      //number of different (length-based) contexts for predicting LZ-77 offsets
    uc RefLengthTableSize;  //log2(external reference length frequency table size)
    uc RefLengthTables;     //number of external reference length frequency tables
    uc RefOffsetTableSize;  //log2(external reference offset frequency table size)
    uc RefOffsetTables;     //number of external reference offset frequency tables
    uc BigMinMatch;         //minimum match length for external references
} IW_UNALIGNED_END;
#define RANGESETTINGS_SIZE 10

#define RANGE_USE_COMBUF_STATIC_TABLES 1
#define RANGE_USE_DZ_STATIC_TABLES 2

#ifndef __GNUC__
#pragma pack (pop)
#endif