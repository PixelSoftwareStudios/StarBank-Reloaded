#include "StormLib.h"

#include <filesystem>
#include <iostream>
#include <string>
#include <vector>

bool add(HANDLE archive, const char *name, const std::string &bytes, DWORD flags = 0, DWORD codec = 0) {
    HANDLE file = nullptr;
    if (!SFileCreateFile(archive, name, 0, static_cast<DWORD>(bytes.size()), 0, flags, &file))
        return false;
    const bool written = SFileWriteFile(file, bytes.data(), static_cast<DWORD>(bytes.size()), codec);
    const bool finished = SFileFinishFile(file);
    return written && finished;
}

bool matches(HANDLE archive, const char *name, const std::string &bytes) {
    HANDLE file = nullptr;
    if (!SFileOpenFileEx(archive, name, SFILE_OPEN_FROM_MPQ, &file))
        return false;
    std::vector<char> output(bytes.size());
    DWORD copied = 0;
    const bool matched = SFileGetFileSize(file, nullptr) == bytes.size() &&
        SFileReadFile(file, output.data(), static_cast<DWORD>(output.size()), &copied, nullptr) &&
        copied == output.size() && bytes == std::string(output.begin(), output.end());
    return SFileCloseFile(file) && matched;
}

int main(int argc, char **argv) {
    if (argc != 2)
        return 1;
    const std::string directory = argv[1];
    const std::string source = directory + "/append-v1-source.s2ma";
    const std::string candidate = directory + "/append-v1-output.s2ma";
    const std::string script = "// Name: Synthetic append fixture\n";
    const std::string first = "Synthetic restored component\n";
    const std::string second = "Synthetic supplemental component\n";
    std::string opaque;
    for (unsigned i = 0; i < 10'000; ++i)
        opaque.push_back(static_cast<char>(i % 53));

    HANDLE archive = nullptr;
    SFILE_CREATE_MPQ options{};
    options.cbSize = sizeof(options);
    options.dwMpqVersion = MPQ_FORMAT_VERSION_1;
    options.dwStreamFlags = STREAM_PROVIDER_FLAT | BASE_PROVIDER_FILE;
    options.dwSectorSize = 4096;
    options.dwMaxFileCount = 16;
    if (!SFileCreateArchive2(source.c_str(), &options, &archive) ||
        !add(archive, "MapScript.galaxy", script) ||
        !add(archive, "Assets\\Opaque.bin", opaque, MPQ_FILE_COMPRESS, MPQ_COMPRESSION_BZIP2) ||
        !SFileFlushArchive(archive) || !SFileCloseArchive(archive))
        return 2;
    std::error_code copy_error;
    if (!std::filesystem::copy_file(source, candidate, copy_error))
        return 3;
    if (!SFileOpenArchive(candidate.c_str(), 0, MPQ_OPEN_NO_HEADER_SEARCH, &archive) ||
        !add(archive, "Fixture\\Restored.txt", first) ||
        !add(archive, "Fixture\\Supplement.txt", second) ||
        !SFileFlushArchive(archive) || !SFileCloseArchive(archive))
        return 4;
    if (!SFileOpenArchive(candidate.c_str(), 0, MPQ_OPEN_READ_ONLY, &archive) ||
        !matches(archive, "MapScript.galaxy", script) ||
        !matches(archive, "Assets\\Opaque.bin", opaque) ||
        !matches(archive, "Fixture\\Restored.txt", first) ||
        !matches(archive, "Fixture\\Supplement.txt", second) ||
        SFileHasFile(archive, "(listfile)") || !SFileCloseArchive(archive))
        return 5;
}
