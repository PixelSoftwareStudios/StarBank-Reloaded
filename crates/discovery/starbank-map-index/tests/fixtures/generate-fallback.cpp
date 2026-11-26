#include "StormLib.h"

#include <iostream>
#include <string>

bool create_fixture(const std::string &path, const char *script, const char *alternate, const char *header) {
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
    const bool written = SFileAddFileEx(archive, script, "MapScript.galaxy", MPQ_FILE_COMPRESS,
        MPQ_COMPRESSION_BZIP2, MPQ_COMPRESSION_BZIP2) &&
        (alternate == nullptr || SFileAddFileEx(archive, alternate, "Base.SC2Data\\MapScript.galaxy", 0, 0, 0)) &&
        SFileAddFileEx(archive, header, "DocumentHeader", 0, 0, 0);
    return SFileCloseArchive(archive) && written;
}

int main(int argc, char **argv) {
    if (argc != 5) {
        return 1;
    }
    const std::string output = argv[4];
    if (!create_fixture(output + "/fallback-script.s2ma", argv[1], argv[2], argv[3]) ||
        !create_fixture(output + "/fallback-header.s2ma", argv[1], nullptr, argv[3])) {
        std::cerr << "fixture creation failed: " << SErrGetLastError() << '\n';
        return 2;
    }
}
