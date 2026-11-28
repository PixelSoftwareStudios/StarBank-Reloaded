#define _POSIX_C_SOURCE 200809L

#include <atspi/atspi.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// The caller owns the Xvfb display, session bus and synthetic bank.
enum {
	MAX_VISITED_NODES = 256,
	MAX_TREE_DEPTH = 24,
	MAX_CONTROL_NAME_BYTES = 64,
	MAX_EXPECTED_TEXT_BYTES = 512,
	PROBE_BUDGET_MS = 2500,
	PROBE_WATCHDOG_SECONDS = 3,
	RPC_TIMEOUT_MS = 400,
	RETRY_INTERVAL_MS = 50,
};

static const char WINDOW_TITLE[] = "StarBank Reloaded";

enum tree_scope {
	DESKTOP,
	DESKTOP_APPLICATION,
	APPLICATION_WINDOW,
	FRAME_CONTENT,
	WEB_CONTENT,
};

struct scan {
	const char *control_name;
	gint64 deadline;
	unsigned visited;
	bool frame_found;
	bool document_found;
	AtspiAccessible *entry;
	GError *rpc_error;
	const char *failure;
};

static void deadline_exceeded(int signal_number)
{
	(void)signal_number;
	static const char message[] = "The AT-SPI probe exceeded its process deadline.\n";
	const ssize_t written = write(STDERR_FILENO, message, sizeof(message) - 1);
	(void)written;
	_exit(EXIT_FAILURE);
}

static bool can_query(struct scan *scan)
{
	if (scan->failure || scan->rpc_error)
		return false;
	const gint64 remaining_ms = (scan->deadline - g_get_monotonic_time()) /
		G_TIME_SPAN_MILLISECOND;
	if (remaining_ms <= 0) {
		scan->failure = "The AT-SPI discovery deadline elapsed.";
		return false;
	}
	// Disable the startup grace period so it cannot extend these RPC deadlines.
	atspi_set_timeout((gint)MIN(remaining_ms, RPC_TIMEOUT_MS), -1);
	return true;
}

static bool name_matches(struct scan *scan, AtspiAccessible *accessible, const char *name)
{
	if (!can_query(scan))
		return false;
	gchar *accessible_name = atspi_accessible_get_name(accessible, &scan->rpc_error);
	const bool matches = !scan->rpc_error && g_strcmp0(accessible_name, name) == 0;
	g_free(accessible_name);
	return matches;
}

static void visit(struct scan *scan, AtspiAccessible *accessible, enum tree_scope scope,
	unsigned depth)
{
	if (depth > MAX_TREE_DEPTH || scan->visited >= MAX_VISITED_NODES) {
		scan->failure = "The scoped AT-SPI tree exceeded the fixture limits.";
		return;
	}
	scan->visited++;
	if (!can_query(scan))
		return;
	const AtspiRole role = scope == DESKTOP ? ATSPI_ROLE_DESKTOP_FRAME :
		atspi_accessible_get_role(accessible, &scan->rpc_error);
	if (scan->rpc_error)
		return;

	enum tree_scope children = scope;
	switch (scope) {
	case DESKTOP:
		children = DESKTOP_APPLICATION;
		break;
	case DESKTOP_APPLICATION:
		if (role != ATSPI_ROLE_APPLICATION)
			return;
		children = APPLICATION_WINDOW;
		break;
	case APPLICATION_WINDOW:
		if (role != ATSPI_ROLE_FRAME || !name_matches(scan, accessible, WINDOW_TITLE))
			return;
		if (scan->frame_found) {
			scan->failure = "The fixture has more than one matching AT-SPI frame.";
			return;
		}
		scan->frame_found = true;
		children = FRAME_CONTENT;
		break;
	case FRAME_CONTENT:
		if (role == ATSPI_ROLE_DOCUMENT_WEB) {
			scan->document_found = true;
			children = WEB_CONTENT;
		}
		break;
	case WEB_CONTENT:
		if (role == ATSPI_ROLE_ENTRY && name_matches(scan, accessible, scan->control_name)) {
			if (scan->entry) {
				scan->failure = "More than one matching entry was exposed by the WebView.";
				return;
			}
			scan->entry = g_object_ref(accessible);
		}
		break;
	}

	if (!can_query(scan))
		return;
	const gint count = atspi_accessible_get_child_count(accessible, &scan->rpc_error);
	if (scan->rpc_error)
		return;
	if (count < 0 || (unsigned)count > MAX_VISITED_NODES - scan->visited) {
		scan->failure = "The scoped AT-SPI object returned an invalid child count.";
		return;
	}
	for (gint index = 0; index < count && can_query(scan); index++) {
		AtspiAccessible *child = atspi_accessible_get_child_at_index(accessible, index,
			&scan->rpc_error);
		if (child) {
			if (!scan->rpc_error)
				visit(scan, child, children, depth + 1);
			g_object_unref(child);
		}
	}
}

static bool verify_text(struct scan *scan, const char *expected)
{
	if (!can_query(scan))
		return false;
	AtspiText *text = atspi_accessible_get_text_iface(scan->entry);
	if (!text) {
		scan->failure = "The WebView entry does not expose the AT-SPI Text interface.";
		return false;
	}

	bool matched = false;
	if (can_query(scan)) {
		const gint count = atspi_text_get_character_count(text, &scan->rpc_error);
		if (!scan->rpc_error && count != g_utf8_strlen(expected, -1)) {
			scan->failure = "The AT-SPI text character count did not match the fixture.";
		} else if (can_query(scan)) {
			gchar *actual = atspi_text_get_text(text, 0, count, &scan->rpc_error);
			matched = can_query(scan) && g_strcmp0(actual, expected) == 0;
			if (!matched && !scan->rpc_error && !scan->failure)
				scan->failure = "The AT-SPI text did not match the exact fixture text.";
			g_free(actual);
		}
	}
	g_object_unref(text);
	return matched;
}

static bool verify_focused_entry(struct scan *scan, bool multiline, const char *expected)
{
	if (!can_query(scan))
		return false;
	AtspiStateSet *states = atspi_accessible_get_state_set(scan->entry);
	if (!states) {
		scan->failure = "The WebView entry did not return an AT-SPI state set.";
		return false;
	}
	const bool focused = atspi_state_set_contains(states, ATSPI_STATE_FOCUSED);
	const AtspiStateType line_state = multiline ? ATSPI_STATE_MULTI_LINE : ATSPI_STATE_SINGLE_LINE;
	const AtspiStateType other_line_state = multiline ? ATSPI_STATE_SINGLE_LINE : ATSPI_STATE_MULTI_LINE;
	const bool expected_states =
		atspi_state_set_contains(states, ATSPI_STATE_FOCUSABLE) &&
		atspi_state_set_contains(states, ATSPI_STATE_EDITABLE) &&
		atspi_state_set_contains(states, ATSPI_STATE_ENABLED) &&
		atspi_state_set_contains(states, ATSPI_STATE_SENSITIVE) &&
		atspi_state_set_contains(states, line_state) &&
		!atspi_state_set_contains(states, other_line_state);
	g_object_unref(states);
	if (!focused)
		return false;
	if (!expected_states) {
		scan->failure = "The focused entry did not have the expected AT-SPI states.";
		return false;
	}
	return verify_text(scan, expected);
}

static bool valid_text(const char *value, size_t maximum)
{
	const size_t length = strlen(value);
	return length > 0 && length <= maximum && g_utf8_validate(value, (gssize)length, NULL);
}

static bool find_control(struct scan *scan, bool multiline, const char *expected)
{
	const char *pending = "The fixture AT-SPI desktop was unavailable.";
	AtspiAccessible *desktop = NULL;
	bool matched = false;
	while (can_query(scan)) {
		if (!desktop) {
			desktop = atspi_get_desktop(0);
			if (desktop)
				atspi_accessible_set_cache_mask(desktop, ATSPI_CACHE_NONE);
		}
		if (desktop) {
			scan->visited = 0;
			scan->frame_found = false;
			scan->document_found = false;
			visit(scan, desktop, DESKTOP, 0);
			if (scan->entry)
				matched = verify_focused_entry(scan, multiline, expected);
			pending = !scan->frame_found ? "The fixture AT-SPI frame was not exposed." :
				!scan->document_found ? "The fixture WebView document was not exposed." :
				!scan->entry ? "The named AT-SPI entry was not exposed." :
				"The named AT-SPI entry was not focused.";
			g_clear_object(&scan->entry);
		}
		if (matched || scan->failure || scan->rpc_error)
			break;
		const gint64 remaining = scan->deadline - g_get_monotonic_time();
		if (remaining < G_TIME_SPAN_MILLISECOND)
			break;
		g_usleep((gulong)MIN(remaining, RETRY_INTERVAL_MS * G_TIME_SPAN_MILLISECOND));
	}
	g_clear_object(&desktop);
	if (!matched && !scan->failure && !scan->rpc_error)
		scan->failure = pending;
	return matched;
}

int main(int argc, char **argv)
{
	if (argc != 4 || !valid_text(argv[1], MAX_CONTROL_NAME_BYTES) ||
		(strcmp(argv[2], "single-line") != 0 && strcmp(argv[2], "multi-line") != 0) ||
		!valid_text(argv[3], MAX_EXPECTED_TEXT_BYTES)) {
		fputs("Expected a control name, single-line or multi-line, and exact fixture text.\n", stderr);
		return EXIT_FAILURE;
	}
	const char *display = getenv("DISPLAY");
	const char *bus = getenv("DBUS_SESSION_BUS_ADDRESS");
	if (!display || !*display || !bus || !*bus) {
		fputs("The AT-SPI probe requires the fixture Xvfb display and private D-Bus session.\n", stderr);
		return EXIT_FAILURE;
	}

	struct sigaction timeout = { .sa_handler = deadline_exceeded };
	if (sigemptyset(&timeout.sa_mask) != 0 || sigaction(SIGALRM, &timeout, NULL) != 0) {
		fputs("Could not set the AT-SPI probe deadline.\n", stderr);
		return EXIT_FAILURE;
	}
	// libatspi's initial GetAddress call has its own D-Bus default timeout.
	alarm(PROBE_WATCHDOG_SECONDS);
	struct scan scan = {
		.control_name = argv[1],
		.deadline = g_get_monotonic_time() + PROBE_BUDGET_MS * G_TIME_SPAN_MILLISECOND,
	};
	if (!can_query(&scan) || atspi_init() != 0) {
		fputs("Could not initialize the fixture AT-SPI registry.\n", stderr);
		return EXIT_FAILURE;
	}
	const bool matched = find_control(&scan, strcmp(argv[2], "multi-line") == 0, argv[3]);
	atspi_exit();
	alarm(0);
	if (!matched) {
		if (scan.rpc_error)
			fprintf(stderr, "AT-SPI query failed: %.240s\n", scan.rpc_error->message);
		else
			fprintf(stderr, "%s\n", scan.failure);
		g_clear_error(&scan.rpc_error);
		return EXIT_FAILURE;
	}
	return EXIT_SUCCESS;
}
