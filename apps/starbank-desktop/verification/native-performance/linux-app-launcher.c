#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <grp.h>
#include <limits.h>
#include <linux/capability.h>
#include <linux/magic.h>
#include <pwd.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/vfs.h>
#include <sys/xattr.h>
#include <unistd.h>

/* Test launcher for a fresh leaf created by linux-cgroup.py on hosted CI. */
static const char application[] = "/usr/bin/starbank-desktop";
static const char cgroup_root[] = "/sys/fs/cgroup";
static const char leaf_prefix[] = "starbank-perf-";
static const char inspector_prefix[] = "127.0.0.1:";
static const char bank_extension[] = ".sc2bank";

enum {
    token_length = 32,
    control_limit = 4096,
    group_limit = 256,
    environment_limit = 17,
    environment_entry_size = PATH_MAX + 64,
};

struct caller {
    uid_t uid;
    gid_t gid;
    size_t group_count;
    gid_t groups[group_limit];
    const char *name;
    char home[PATH_MAX];
};

struct environment {
    size_t count;
    char entries[environment_limit][environment_entry_size];
    char *envp[environment_limit + 1];
};

static int fail(const char *message)
{
    fprintf(stderr, "StarBank performance launcher: %s\n", message);
    return EXIT_FAILURE;
}

static bool decimal(const char *text, uint64_t *value)
{
    if (!text || !*text || (text[0] == '0' && text[1] != '\0'))
        return false;

    uint64_t result = 0;
    for (; *text; ++text) {
        if (*text < '0' || *text > '9')
            return false;
        unsigned digit = (unsigned)(*text - '0');
        if (result > (UINT64_MAX - digit) / 10)
            return false;
        result = result * 10 + digit;
    }
    *value = result;
    return true;
}

static bool valid_token(const char *token)
{
    if (strlen(token) != token_length)
        return false;
    for (const char *p = token; *p; ++p) {
        if (!((*p >= '0' && *p <= '9') || (*p >= 'a' && *p <= 'f')))
            return false;
    }
    return true;
}

static bool valid_value(const char *value)
{
    if (!value || !*value || strnlen(value, PATH_MAX) >= PATH_MAX)
        return false;
    for (const unsigned char *p = (const unsigned char *)value; *p; ++p) {
        if (*p < 32 || *p == 127)
            return false;
    }
    return true;
}

static bool absolute_path(const char *value)
{
    return valid_value(value) && value[0] == '/' && value[1] != '\0';
}

static bool bank_path(const char *value)
{
    if (!absolute_path(value))
        return false;
    size_t length = strlen(value);
    size_t suffix = sizeof bank_extension - 1;
    return length > suffix && strcasecmp(value + length - suffix, bank_extension) == 0;
}

static bool fixture_display(const char *value)
{
    if (!valid_value(value) || *value++ != ':' || *value < '0' || *value > '9')
        return false;
    value += strspn(value, "0123456789");
    if (*value == '.' && value[1] >= '0' && value[1] <= '9')
        value += 1 + strspn(value + 1, "0123456789");
    return !*value;
}

static bool fixture_bus(const char *address)
{
    static const char path[] = "unix:path=";
    static const char abstract[] = "unix:abstract=";
    if (!valid_value(address) || strchr(address, ';'))
        return false;
    return (strncmp(address, path, sizeof path - 1) == 0 &&
            address[sizeof path - 1] == '/') ||
           (strncmp(address, abstract, sizeof abstract - 1) == 0 &&
            address[sizeof abstract - 1] != '\0' &&
            address[sizeof abstract - 1] != ',');
}

static bool fixture_inspector(const char *address)
{
    uint64_t port;
    size_t length = sizeof inspector_prefix - 1;
    return valid_value(address) && strncmp(address, inspector_prefix, length) == 0 &&
           decimal(address + length, &port) && port > 0 && port <= UINT16_MAX;
}

static bool caller_from_sudo(struct caller *caller)
{
    uint64_t uid, gid;
    const char *name = getenv("SUDO_USER");
    if (!valid_value(name) || !decimal(getenv("SUDO_UID"), &uid) ||
        !decimal(getenv("SUDO_GID"), &gid) || uid == 0 || gid == 0 ||
        (uint64_t)(uid_t)uid != uid || (uint64_t)(gid_t)gid != gid ||
        (uid_t)uid == (uid_t)-1 || (gid_t)gid == (gid_t)-1)
        return false;

    struct passwd *identity = getpwuid((uid_t)uid);
    if (!identity || strcmp(name, identity->pw_name) != 0 ||
        identity->pw_gid != (gid_t)gid || !absolute_path(identity->pw_dir))
        return false;

    caller->uid = (uid_t)uid;
    caller->gid = (gid_t)gid;
    caller->name = name;
    memcpy(caller->home, identity->pw_dir, strlen(identity->pw_dir) + 1);

    int count = group_limit;
    if (getgrouplist(name, caller->gid, caller->groups, &count) < 0 ||
        count <= 0 || count > group_limit)
        return false;
    caller->group_count = 0;
    for (int i = 0; i < count; ++i) {
        if (caller->groups[i] != 0)
            caller->groups[caller->group_count++] = caller->groups[i];
    }
    return true;
}

static bool add_environment(struct environment *target, const char *key, const char *value)
{
    if (!valid_value(value) || target->count == environment_limit)
        return false;
    char *entry = target->entries[target->count];
    int length = snprintf(entry, environment_entry_size, "%s=%s", key, value);
    if (length < 0 || length >= environment_entry_size)
        return false;
    target->envp[target->count++] = entry;
    target->envp[target->count] = NULL;
    return true;
}

static bool exact_environment(const char *key, const char *expected)
{
    const char *value = getenv(key);
    return value && strcmp(value, expected) == 0;
}

static bool fixture_environment(struct environment *target, const struct caller *caller)
{
    const char *inspector = getenv("WEBKIT_INSPECTOR_SERVER");
    const char *overlay = getenv("GTK_OVERLAY_SCROLLING");
    const char *display = getenv("DISPLAY");
    const char *bus = getenv("DBUS_SESSION_BUS_ADDRESS");
    if (!fixture_inspector(inspector) || !fixture_display(display) || !fixture_bus(bus) ||
        !overlay || (strcmp(overlay, "0") != 0 && strcmp(overlay, "1") != 0) ||
        !exact_environment("TAURI_WEBVIEW_AUTOMATION", "true") ||
        !exact_environment("GTK_USE_PORTAL", "0") ||
        !exact_environment("G_DEBUG", "fatal-criticals") ||
        !exact_environment("LC_ALL", "C.UTF-8"))
        return false;

    if (!add_environment(target, "WEBKIT_INSPECTOR_SERVER", inspector) ||
        !add_environment(target, "GTK_OVERLAY_SCROLLING", overlay) ||
        !add_environment(target, "DISPLAY", display) ||
        !add_environment(target, "DBUS_SESSION_BUS_ADDRESS", bus) ||
        !add_environment(target, "TAURI_WEBVIEW_AUTOMATION", "true") ||
        !add_environment(target, "GTK_USE_PORTAL", "0") ||
        !add_environment(target, "G_DEBUG", "fatal-criticals") ||
        !add_environment(target, "LC_ALL", "C.UTF-8") ||
        !add_environment(target, "HOME", caller->home) ||
        !add_environment(target, "USER", caller->name) ||
        !add_environment(target, "LOGNAME", caller->name) ||
        !add_environment(target, "PATH", "/usr/bin:/bin"))
        return false;

    const char *const required_paths[] = {
        "XAUTHORITY", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME",
    };
    for (size_t i = 0; i < sizeof required_paths / sizeof *required_paths; ++i) {
        const char *value = getenv(required_paths[i]);
        if (!absolute_path(value) || !add_environment(target, required_paths[i], value))
            return false;
    }
    const char *runtime = getenv("XDG_RUNTIME_DIR");
    return !runtime || (absolute_path(runtime) && add_environment(target, "XDG_RUNTIME_DIR", runtime));
}

static bool installed_application(void)
{
    int fd = open(application, O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC);
    if (fd < 0)
        return false;
    struct stat info;
    bool valid = fstat(fd, &info) == 0 && S_ISREG(info.st_mode) && info.st_uid == 0 &&
                 (info.st_mode & S_IXOTH) &&
                 !(info.st_mode & (S_ISUID | S_ISGID | S_IWGRP | S_IWOTH));
    if (valid)
        valid = fgetxattr(fd, "security.capability", NULL, 0) < 0 && errno == ENODATA;
    return close(fd) == 0 && valid;
}

static bool control_text(int leaf, const char *name, char text[static control_limit + 1])
{
    int fd = openat(leaf, name, O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC);
    if (fd < 0)
        return false;
    struct stat info;
    bool valid = fstat(fd, &info) == 0 && S_ISREG(info.st_mode) && info.st_uid == 0;
    size_t used = 0;
    while (valid) {
        ssize_t count = read(fd, text + used, control_limit + 1 - used);
        if (count < 0 && errno == EINTR)
            continue;
        if (count <= 0) {
            valid = count == 0;
            break;
        }
        used += (size_t)count;
        valid = used <= control_limit;
    }
    if (close(fd) != 0 || !valid || memchr(text, '\0', used))
        return false;
    text[used] = '\0';
    return true;
}

static bool zero_field(const char *text, const char *key)
{
    size_t key_length = strlen(key);
    bool found = false;
    for (const char *line = text; *line;) {
        const char *end = strchr(line, '\n');
        if (!end)
            return false;
        size_t length = (size_t)(end - line);
        if (length > key_length && strncmp(line, key, key_length) == 0 &&
            line[key_length] == ' ') {
            if (found || length != key_length + 2 || line[key_length + 1] != '0')
                return false;
            found = true;
        }
        line = end + 1;
    }
    return found;
}

static bool fresh_leaf(int leaf)
{
    char text[control_limit + 1];
    return control_text(leaf, "cpu.stat", text) && zero_field(text, "usage_usec") &&
           zero_field(text, "user_usec") && zero_field(text, "system_usec") &&
           control_text(leaf, "memory.current", text) && strcmp(text, "0\n") == 0 &&
           control_text(leaf, "memory.peak", text) && strcmp(text, "0\n") == 0 &&
           control_text(leaf, "cgroup.events", text) && zero_field(text, "populated") &&
           zero_field(text, "frozen") && control_text(leaf, "cgroup.procs", text) && !*text;
}

static bool join_leaf(const char *token, uint64_t device, uint64_t inode)
{
    char name[sizeof leaf_prefix + token_length];
    memcpy(name, leaf_prefix, sizeof leaf_prefix - 1);
    memcpy(name + sizeof leaf_prefix - 1, token, token_length + 1);
    int root = open(cgroup_root, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    if (root < 0)
        return false;

    struct statfs filesystem;
    int leaf = -1;
    if (fstatfs(root, &filesystem) == 0 && filesystem.f_type == CGROUP2_SUPER_MAGIC)
        leaf = openat(root, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    if (close(root) != 0 || leaf < 0) {
        if (leaf >= 0)
            (void)close(leaf);
        return false;
    }

    struct stat info;
    int procs = -1;
    if (fstat(leaf, &info) == 0 && (uint64_t)info.st_dev == device &&
        (uint64_t)info.st_ino == inode && info.st_uid == 0 &&
        !(info.st_mode & (S_IWGRP | S_IWOTH)) && fresh_leaf(leaf))
        procs = openat(leaf, "cgroup.procs", O_WRONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC);
    if (close(leaf) != 0 || procs < 0) {
        if (procs >= 0)
            (void)close(procs);
        return false;
    }

    // The kernel interprets zero as this process. The writer never accepts a PID.
    bool moved = fstat(procs, &info) == 0 && S_ISREG(info.st_mode) && info.st_uid == 0 &&
                 write(procs, "0\n", 2) == 2;
    return close(procs) == 0 && moved;
}

static int compare_groups(const void *left, const void *right)
{
    gid_t a = *(const gid_t *)left;
    gid_t b = *(const gid_t *)right;
    return (a > b) - (a < b);
}

static bool has_no_capabilities(void)
{
    struct __user_cap_header_struct header = {.version = _LINUX_CAPABILITY_VERSION_3};
    struct __user_cap_data_struct data[_LINUX_CAPABILITY_U32S_3] = {0};
    if (syscall(SYS_capget, &header, data) != 0)
        return false;
    // Linux requires every ambient capability to be both permitted and inheritable.
    for (size_t i = 0; i < _LINUX_CAPABILITY_U32S_3; ++i) {
        if (data[i].effective || data[i].permitted || data[i].inheritable)
            return false;
    }
    return true;
}

static bool drop_privileges(struct caller *caller)
{
    if (setgroups(caller->group_count, caller->groups) != 0 ||
        setresgid(caller->gid, caller->gid, caller->gid) != 0 ||
        setresuid(caller->uid, caller->uid, caller->uid) != 0)
        return false;

    uid_t real_uid, effective_uid, saved_uid;
    gid_t real_gid, effective_gid, saved_gid, groups[group_limit];
    int count = getgroups(group_limit, groups);
    if (getresuid(&real_uid, &effective_uid, &saved_uid) != 0 ||
        getresgid(&real_gid, &effective_gid, &saved_gid) != 0 ||
        real_uid != caller->uid || effective_uid != caller->uid || saved_uid != caller->uid ||
        real_gid != caller->gid || effective_gid != caller->gid || saved_gid != caller->gid ||
        count < 0 || (size_t)count != caller->group_count)
        return false;

    qsort(caller->groups, caller->group_count, sizeof *caller->groups, compare_groups);
    qsort(groups, (size_t)count, sizeof *groups, compare_groups);
    return memcmp(groups, caller->groups, (size_t)count * sizeof *groups) == 0 &&
           has_no_capabilities();
}

int main(int argc, char **argv)
{
    uint64_t device, inode;
    if ((argc != 4 && argc != 5) || !valid_token(argv[1]) ||
        !decimal(argv[2], &device) || !decimal(argv[3], &inode) || inode == 0 ||
        (argc == 5 && !bank_path(argv[4])))
        return fail("expected a leaf token, device, inode, and optional absolute bank path");

    struct caller caller;
    if (geteuid() != 0 || !caller_from_sudo(&caller))
        return fail("expected the non-root runner identity from sudo");
    struct environment environment = {0};
    if (!fixture_environment(&environment, &caller))
        return fail("invalid WebDriver fixture environment");
    if (!installed_application())
        return fail("could not verify the installed application");
    if (close_range(STDERR_FILENO + 1, UINT_MAX, 0) != 0)
        return fail("could not close inherited file descriptors");
    if (!join_leaf(argv[1], device, inode))
        return fail("could not enter the identified fresh cgroup");
    if (!drop_privileges(&caller))
        return fail("could not verify the runner credentials after dropping privileges");

    char *const arguments[] = {(char *)application, argc == 5 ? argv[4] : NULL, NULL};
    execve(application, arguments, environment.envp);
    return fail("could not exec the installed application");
}
