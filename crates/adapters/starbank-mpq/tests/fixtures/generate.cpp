#include "StormLib.h"

#include <array>
#include <fstream>
#include <iostream>
#include <iterator>
#include <string>
#include <vector>

enum class Contents { Metadata, AlternateCodec, EmptyArchive, EmptyScript };

struct Fixture {
    const char *filename;
    DWORD version;
    bool listfile;
    Contents contents;
};

static bool matches_payload(const char *path, const char *entry, const std::vector<char> &expected) {
    HANDLE archive = nullptr;
    HANDLE file = nullptr;
    if (!SFileOpenArchive(path, 0, MPQ_OPEN_READ_ONLY, &archive)) {
        return false;
    }
    if (!SFileOpenFileEx(archive, entry, SFILE_OPEN_FROM_MPQ, &file)) {
        SFileCloseArchive(archive);
        return false;
    }

    std::vector<char> data(expected.size());
    DWORD actual = 0;
    const auto size = static_cast<DWORD>(expected.size());
    bool result = SFileGetFileSize(file, nullptr) == size &&
        (expected.empty() || SFileReadFile(file, data.data(), size, &actual, nullptr)) &&
        actual == size && data == expected;
    SFileCloseFile(file);
    SFileCloseArchive(archive);
    return result;
}

static bool metadata_absent(const char *path, const char *entry) {
    HANDLE archive = nullptr;
    if (!SFileOpenArchive(path, 0, MPQ_OPEN_READ_ONLY, &archive)) {
        return false;
    }
    const bool absent = !SFileHasFile(archive, entry);
    return SFileCloseArchive(archive) && absent;
}

int main(int argc, char **argv) {
    if (argc != 4) {
        return 1;
    }

    const std::array fixtures{
        Fixture{"v1-no-list.s2ma", MPQ_FORMAT_VERSION_1, false, Contents::Metadata},
        Fixture{"v1-list.s2ma", MPQ_FORMAT_VERSION_1, true, Contents::AlternateCodec},
        Fixture{"v2-list.s2ma", MPQ_FORMAT_VERSION_2, true, Contents::Metadata},
        Fixture{"v4-no-list.s2ma", MPQ_FORMAT_VERSION_4, false, Contents::Metadata},
        Fixture{"empty-classic.s2ma", MPQ_FORMAT_VERSION_1, false, Contents::EmptyArchive},
        Fixture{"empty-script.s2ma", MPQ_FORMAT_VERSION_1, false, Contents::EmptyScript},
    };
    const std::array<std::pair<const char *, DWORD>, 3> entries{{
        {"MapScript.galaxy", MPQ_FILE_COMPRESS},
        {"DocumentHeader", 0},
        {"Base.SC2Data\\MapScript.galaxy", MPQ_FILE_COMPRESS | MPQ_FILE_SINGLE_UNIT},
    }};
    std::ifstream fixture(argv[1], std::ios::binary);
    const std::vector<char> expected{std::istreambuf_iterator<char>(fixture), {}};
    if (expected.empty()) {
        return 2;
    }

    for (const auto &setup : fixtures) {
        auto path = std::string(argv[3]) + "/" + setup.filename;
        HANDLE archive = nullptr;
        SFILE_CREATE_MPQ options{};
        options.cbSize = sizeof(options);
        options.dwMpqVersion = setup.version;
        options.dwStreamFlags = STREAM_PROVIDER_FLAT | BASE_PROVIDER_FILE;
        options.dwFileFlags1 = setup.listfile ? MPQ_FILE_DEFAULT_INTERNAL : 0;
        options.dwSectorSize = 4096;
        options.dwRawChunkSize = setup.version == MPQ_FORMAT_VERSION_4 ? 16384 : 0;
        options.dwMaxFileCount = 16;
        if (!SFileCreateArchive2(path.c_str(), &options, &archive)) {
            std::cerr << path << " create " << SErrGetLastError() << '\n';
            return 3;
        }
        const auto include = [&](const std::string &name) {
            switch (setup.contents) {
            case Contents::Metadata: return true;
            case Contents::AlternateCodec: return name != "DocumentHeader";
            case Contents::EmptyArchive: return false;
            case Contents::EmptyScript: return name == "MapScript.galaxy";
            }
            return false;
        };
        const char *source = setup.contents == Contents::EmptyScript ? argv[2] : argv[1];
        for (const auto &[name, flags] : entries) {
            if (!include(name)) {
                continue;
            }
            const DWORD codec = setup.contents == Contents::AlternateCodec &&
                std::string(name) == "MapScript.galaxy" ? MPQ_COMPRESSION_BZIP2 : MPQ_COMPRESSION_ZLIB;
            if (!SFileAddFileEx(archive, source, name, flags, codec, codec)) {
                std::cerr << path << " add " << name << ' ' << SErrGetLastError() << '\n';
                SFileCloseArchive(archive);
                return 4;
            }
        }
        if (!SFileCloseArchive(archive)) {
            return 5;
        }
        const std::vector<char> empty;
        const auto &expected_contents = setup.contents == Contents::EmptyScript ? empty : expected;
        for (const auto &[name, flags] : entries) {
            bool matches = include(name)
                ? matches_payload(path.c_str(), name, expected_contents)
                : metadata_absent(path.c_str(), name);
            if (!matches) {
                std::cerr << path << " read " << name << ' ' << SErrGetLastError() << '\n';
                return 6;
            }
            std::cout << path << " StormLib pass " << name << '\n';
        }
    }
}
