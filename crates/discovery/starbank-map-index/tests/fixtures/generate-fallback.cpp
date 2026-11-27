#include "StormLib.h"

#include <iostream>
#include <string>

enum class Storage { Stored, Bzip2, Zlib };

bool add_member(HANDLE archive, const char *source, const char *name, Storage storage) {
    const DWORD flags = storage == Storage::Stored ? 0 : MPQ_FILE_COMPRESS;
    const DWORD codec = storage == Storage::Bzip2 ? MPQ_COMPRESSION_BZIP2 :
        storage == Storage::Zlib ? MPQ_COMPRESSION_ZLIB : 0;
    return SFileAddFileEx(archive, source, name, flags, codec, codec);
}

bool create_fixture(const std::string &path, const char *script, const char *alternate, const char *header,
                    Storage script_storage = Storage::Bzip2, Storage header_storage = Storage::Stored) {
    HANDLE archive = nullptr;
    SFILE_CREATE_MPQ options{};
    options.cbSize = sizeof(options);
    options.dwMpqVersion = MPQ_FORMAT_VERSION_1;
    options.dwStreamFlags = STREAM_PROVIDER_FLAT | BASE_PROVIDER_FILE;
    options.dwSectorSize = 4096;
    options.dwMaxFileCount = 16;
    if (!SFileCreateArchive2(path.c_str(), &options, &archive)) {
        return false;
    }
    const bool written = add_member(archive, script, "MapScript.galaxy", script_storage) &&
        (alternate == nullptr || add_member(archive, alternate, "Base.SC2Data\\MapScript.galaxy", Storage::Stored)) &&
        (header == nullptr || add_member(archive, header, "DocumentHeader", header_storage));
    return SFileCloseArchive(archive) && written;
}

int main(int argc, char **argv) {
    if (argc != 5 && argc != 8 && argc != 10) {
        return 1;
    }
    const std::string output = argv[4];
    if (!create_fixture(output + "/fallback-script.s2ma", argv[1], argv[2], argv[3]) ||
        !create_fixture(output + "/fallback-header.s2ma", argv[1], nullptr, argv[3]) ||
        (argc >= 8 &&
         (!create_fixture(output + "/invalid-script-with-alternate.s2ma", argv[5], argv[2], argv[3], Storage::Stored) ||
          !create_fixture(output + "/invalid-script-with-header.s2ma", argv[5], nullptr, argv[3], Storage::Stored) ||
          !create_fixture(output + "/script-reference-limit.s2ma", argv[6], nullptr, argv[3], Storage::Stored) ||
          !create_fixture(output + "/reference-limit-with-alternate.s2ma", argv[6], argv[7], nullptr, Storage::Stored))) ||
        (argc == 10 &&
         !create_fixture(output + "/member-local-recovery.s2ma", argv[8], nullptr, argv[9], Storage::Zlib, Storage::Zlib))) {
        std::cerr << "fixture creation failed: " << SErrGetLastError() << '\n';
        return 2;
    }
}
