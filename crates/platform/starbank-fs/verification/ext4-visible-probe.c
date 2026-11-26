#define _GNU_SOURCE
#include <endian.h>
#include <errno.h>
#include <fcntl.h>
#include <linux/posix_acl.h>
#include <linux/posix_acl_xattr.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/xattr.h>
#include <unistd.h>

#define REQUIRE(result) do { if (!(result)) { \
	fprintf(stderr, "probe %d: %s (errno=%d: %s)\n", __LINE__, #result, errno, strerror(errno)); \
	exit(EXIT_FAILURE); \
} } while (0)

static const char user_attribute[] = "user.starbank.test";
static const char acl_attribute[] = "system.posix_acl_access";
static const char trusted_attribute[] = "trusted.starbank.test";
static const char old_data[] = "<Bank version=\"1\"/>";
static const char new_data[] = "<Bank version=\"2\"/>";
static const char visible_data[] = "synthetic metadata";

static int open_file(int parent, const char *name, int access) {
	int file = openat(parent, name, access | O_CLOEXEC | O_NOFOLLOW, 0600);
	REQUIRE(file >= 0);
	return file;
}

static void expect_bytes(int file, const char *expected) {
	char data[64];
	ssize_t count = pread(file, data, sizeof(data), 0);
	REQUIRE(count == (ssize_t)strlen(expected));
	REQUIRE(memcmp(data, expected, (size_t)count) == 0);
}

int main(void) {
	struct test_acl {
		struct posix_acl_xattr_header header;
		struct posix_acl_xattr_entry entries[5];
	};
	const uid_t test_reader = 65533;
	const struct test_acl acl = {
		.header = {htole32(POSIX_ACL_XATTR_VERSION)},
		.entries = {
			{htole16(ACL_USER_OBJ), htole16(ACL_READ | ACL_WRITE), htole32(ACL_UNDEFINED_ID)},
			{htole16(ACL_USER), htole16(ACL_READ), htole32(test_reader)},
			{htole16(ACL_GROUP_OBJ), htole16(0), htole32(ACL_UNDEFINED_ID)},
			{htole16(ACL_MASK), htole16(ACL_READ), htole32(ACL_UNDEFINED_ID)},
			{htole16(ACL_OTHER), htole16(0), htole32(ACL_UNDEFINED_ID)}
		}
	};

	int dir = open("/probe", O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
	REQUIRE(dir >= 0);
	int target = open_file(dir, "Fixture.SC2Bank", O_CREAT | O_EXCL | O_RDWR);
	int stage = open_file(dir, ".pending", O_CREAT | O_EXCL | O_RDWR);

	REQUIRE(write(target, old_data, sizeof(old_data) - 1) == sizeof(old_data) - 1);
	REQUIRE(write(stage, new_data, sizeof(new_data) - 1) == sizeof(new_data) - 1);
	REQUIRE(fsetxattr(target, user_attribute, visible_data, sizeof(visible_data), XATTR_CREATE) == 0);
	REQUIRE(fsetxattr(target, acl_attribute, &acl, sizeof(acl), XATTR_CREATE) == 0);

	errno = 0;
	REQUIRE(fsetxattr(target, trusted_attribute, visible_data, sizeof(visible_data), XATTR_CREATE) == -1);
	REQUIRE(errno == EPERM);
	puts("trusted namespace insertion refused without privileges");

	struct stat old_stat, stage_stat, published_stat, displaced_stat;
	REQUIRE(fstat(target, &old_stat) == 0);
	REQUIRE((old_stat.st_mode & 0777) == 0640);
	REQUIRE(fchmod(stage, old_stat.st_mode & 0777) == 0);

	char observed_user[sizeof(visible_data)];
	char observed_acl[sizeof(acl)];
	REQUIRE(fgetxattr(target, user_attribute, observed_user, sizeof(observed_user)) == sizeof(observed_user));
	REQUIRE(fgetxattr(target, acl_attribute, observed_acl, sizeof(observed_acl)) == sizeof(observed_acl));
	REQUIRE(fsetxattr(stage, user_attribute, observed_user, sizeof(observed_user), XATTR_CREATE) == 0);
	REQUIRE(fsetxattr(stage, acl_attribute, observed_acl, sizeof(observed_acl), XATTR_CREATE) == 0);
	REQUIRE(fsync(target) == 0);
	REQUIRE(fsync(stage) == 0);
	REQUIRE(fstat(stage, &stage_stat) == 0);
	REQUIRE(close(target) == 0);
	REQUIRE(close(stage) == 0);

	REQUIRE(renameat2(dir, ".pending", dir, "Fixture.SC2Bank", RENAME_EXCHANGE) == 0);
	REQUIRE(fsync(dir) == 0);
	int published = open_file(dir, "Fixture.SC2Bank", O_RDONLY);
	int displaced = open_file(dir, ".pending", O_RDONLY);

	REQUIRE(fstat(published, &published_stat) == 0);
	REQUIRE(fstat(displaced, &displaced_stat) == 0);
	REQUIRE(published_stat.st_ino == stage_stat.st_ino);
	REQUIRE(displaced_stat.st_ino == old_stat.st_ino);
	REQUIRE(published_stat.st_uid == old_stat.st_uid);
	REQUIRE(published_stat.st_gid == old_stat.st_gid);
	REQUIRE(published_stat.st_mode == old_stat.st_mode);
	expect_bytes(published, new_data);
	expect_bytes(displaced, old_data);
	char actual_user[sizeof(visible_data)];
	char actual_acl[sizeof(acl)];
	REQUIRE(fgetxattr(published, user_attribute, actual_user, sizeof(actual_user)) == sizeof(actual_user));
	REQUIRE(fgetxattr(published, acl_attribute, actual_acl, sizeof(actual_acl)) == sizeof(actual_acl));
	REQUIRE(memcmp(observed_user, actual_user, sizeof(actual_user)) == 0);
	REQUIRE(memcmp(observed_acl, actual_acl, sizeof(actual_acl)) == 0);

	REQUIRE(close(published) == 0);
	REQUIRE(close(displaced) == 0);
	REQUIRE(close(dir) == 0);
	puts("exchange, file/directory fsync, read-back identity, mode, owner, ACL and user xattr passed");
	return EXIT_SUCCESS;
}
