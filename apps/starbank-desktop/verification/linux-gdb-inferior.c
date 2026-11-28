#define _POSIX_C_SOURCE 200809L

#include <inttypes.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>

// GDB's exec-wrapper restores only its own parent's existing driver group.
static const char application[] = "/usr/bin/starbank-desktop";
static const char group_environment[] = "STARBANK_GDB_DRIVER_GROUP";
static const char joined_marker[] = "diagnostic inferior rejoined driver group\n";

int main(int argc, char **argv)
{
	if (argc < 2 || strcmp(argv[1], application) != 0)
		return EXIT_FAILURE;

	const pid_t parent = getppid();
	if (parent <= 1)
		return EXIT_FAILURE;
	const pid_t parent_group = getpgid(parent);
	if (parent_group <= 0)
		return EXIT_FAILURE;

	// The bit width is an upper bound for the decimal digit count.
	char expected[sizeof(intmax_t) * CHAR_BIT + 1];
	const int length = snprintf(expected, sizeof expected, "%jd", (intmax_t)parent_group);
	const char *controller_group = getenv(group_environment);
	if (length <= 0 || (size_t)length >= sizeof expected || !controller_group ||
		strcmp(controller_group, expected) != 0)
		return EXIT_FAILURE;

	// The target is derived from the live parent, never from an input PID.
	if (setpgid(0, parent_group) != 0 || getpgrp() != parent_group ||
		getppid() != parent || getpgid(parent) != parent_group)
		return EXIT_FAILURE;

	if (fputs(joined_marker, stdout) == EOF || fflush(stdout) != 0)
		return EXIT_FAILURE;
	execv(application, &argv[1]);
	return EXIT_FAILURE;
}
