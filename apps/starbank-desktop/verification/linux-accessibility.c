#define _POSIX_C_SOURCE 200809L

#include <atspi/atspi.h>
#include <signal.h>
#include <stdbool.h>
#include <stdatomic.h>
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

enum probe_operation {
	INITIALIZE,
	GET_DESKTOP,
	SET_CACHE_MASK,
	GET_ROLE,
	GET_NAME,
	GET_CHILD_COUNT,
	GET_CHILD,
	GET_STATES,
	GET_TEXT_INTERFACE,
	GET_CHARACTER_COUNT,
	GET_TEXT,
	RELEASE_REFERENCE,
	WAIT_FOR_REGISTRATION,
	DISCONNECT_AFTER_MATCH,
	DISCONNECT_AFTER_FAILURE,
};

static atomic_int current_operation = INITIALIZE;
_Static_assert(ATOMIC_INT_LOCK_FREE == 2, "The AT-SPI watchdog requires lock-free operation tracking.");

struct operation_description {
	const char *text;
	size_t length;
};

#define OPERATION_DESCRIPTION(text) ((struct operation_description){ (text), sizeof(text) - 1 })

// Only selects string literals. The watchdog does not format or allocate text.
static struct operation_description describe_operation(enum probe_operation operation)
{
	switch (operation) {
	case INITIALIZE:
		return OPERATION_DESCRIPTION("atspi_init");
	case GET_DESKTOP:
		return OPERATION_DESCRIPTION("atspi_get_desktop");
	case SET_CACHE_MASK:
		return OPERATION_DESCRIPTION("atspi_accessible_set_cache_mask");
	case GET_ROLE:
		return OPERATION_DESCRIPTION("atspi_accessible_get_role");
	case GET_NAME:
		return OPERATION_DESCRIPTION("atspi_accessible_get_name");
	case GET_CHILD_COUNT:
		return OPERATION_DESCRIPTION("atspi_accessible_get_child_count");
	case GET_CHILD:
		return OPERATION_DESCRIPTION("atspi_accessible_get_child_at_index");
	case GET_STATES:
		return OPERATION_DESCRIPTION("atspi_accessible_get_state_set");
	case GET_TEXT_INTERFACE:
		return OPERATION_DESCRIPTION("atspi_accessible_get_text_iface");
	case GET_CHARACTER_COUNT:
		return OPERATION_DESCRIPTION("atspi_text_get_character_count");
	case GET_TEXT:
		return OPERATION_DESCRIPTION("atspi_text_get_text");
	case RELEASE_REFERENCE:
		return OPERATION_DESCRIPTION("g_object_unref");
	case WAIT_FOR_REGISTRATION:
		return OPERATION_DESCRIPTION("wait for registration or focus");
	case DISCONNECT_AFTER_MATCH:
		return OPERATION_DESCRIPTION("atspi_exit after a confirmed match");
	case DISCONNECT_AFTER_FAILURE:
		return OPERATION_DESCRIPTION("atspi_exit after a failed observation");
	}
	return OPERATION_DESCRIPTION("unknown operation");
}

#undef OPERATION_DESCRIPTION

static void enter_operation(enum probe_operation operation)
{
	atomic_store_explicit(&current_operation, operation, memory_order_relaxed);
}

static enum probe_operation observed_operation(void)
{
	return (enum probe_operation)atomic_load_explicit(&current_operation, memory_order_relaxed);
}

static void release_reference(gpointer object)
{
	if (!object)
		return;
	const enum probe_operation previous = observed_operation();
	enter_operation(RELEASE_REFERENCE);
	g_object_unref(object);
	enter_operation(previous);
}

static void deadline_exceeded(int signal_number)
{
	(void)signal_number;
	const struct operation_description operation = describe_operation(observed_operation());
	const char prefix[] = "The AT-SPI probe exceeded its process deadline during ";
	const char suffix[] = ".\n";
	const ssize_t prefix_written = write(STDERR_FILENO, prefix, sizeof(prefix) - 1);
	const ssize_t operation_written = write(STDERR_FILENO, operation.text, operation.length);
	const ssize_t suffix_written = write(STDERR_FILENO, suffix, sizeof(suffix) - 1);
	(void)prefix_written;
	(void)operation_written;
	(void)suffix_written;
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
	enter_operation(GET_NAME);
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
	AtspiRole role = ATSPI_ROLE_DESKTOP_FRAME;
	if (scope != DESKTOP) {
		enter_operation(GET_ROLE);
		role = atspi_accessible_get_role(accessible, &scan->rpc_error);
	}
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
	enter_operation(GET_CHILD_COUNT);
	const gint count = atspi_accessible_get_child_count(accessible, &scan->rpc_error);
	if (scan->rpc_error)
		return;
	if (count < 0 || (unsigned)count > MAX_VISITED_NODES - scan->visited) {
		scan->failure = "The scoped AT-SPI object returned an invalid child count.";
		return;
	}
	for (gint index = 0; index < count && can_query(scan); index++) {
		enter_operation(GET_CHILD);
		AtspiAccessible *child = atspi_accessible_get_child_at_index(accessible, index,
			&scan->rpc_error);
		if (child) {
			if (!scan->rpc_error)
				visit(scan, child, children, depth + 1);
			release_reference(child);
		}
	}
}

static bool verify_text(struct scan *scan, const char *expected)
{
	if (!can_query(scan))
		return false;
	enter_operation(GET_TEXT_INTERFACE);
	AtspiText *text = atspi_accessible_get_text_iface(scan->entry);
	if (!text) {
		scan->failure = "The WebView entry does not expose the AT-SPI Text interface.";
		return false;
	}

	bool matched = false;
	if (can_query(scan)) {
		enter_operation(GET_CHARACTER_COUNT);
		const gint count = atspi_text_get_character_count(text, &scan->rpc_error);
		if (!scan->rpc_error && count != g_utf8_strlen(expected, -1)) {
			scan->failure = "The AT-SPI text character count did not match the fixture.";
		} else if (can_query(scan)) {
			enter_operation(GET_TEXT);
			gchar *actual = atspi_text_get_text(text, 0, count, &scan->rpc_error);
			matched = can_query(scan) && g_strcmp0(actual, expected) == 0;
			if (!matched && !scan->rpc_error && !scan->failure)
				scan->failure = "The AT-SPI text did not match the exact fixture text.";
			g_free(actual);
		}
	}
	release_reference(text);
	return matched;
}

static bool verify_focused_entry(struct scan *scan, bool multiline, const char *expected)
{
	if (!can_query(scan))
		return false;
	enter_operation(GET_STATES);
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
	release_reference(states);
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
			enter_operation(GET_DESKTOP);
			desktop = atspi_get_desktop(0);
			if (desktop) {
				enter_operation(SET_CACHE_MASK);
				atspi_accessible_set_cache_mask(desktop, ATSPI_CACHE_NONE);
			}
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
			release_reference(scan->entry);
			scan->entry = NULL;
		}
		if (matched || scan->failure || scan->rpc_error)
			break;
		const gint64 remaining = scan->deadline - g_get_monotonic_time();
		if (remaining < G_TIME_SPAN_MILLISECOND)
			break;
		enter_operation(WAIT_FOR_REGISTRATION);
		g_usleep((gulong)MIN(remaining, RETRY_INTERVAL_MS * G_TIME_SPAN_MILLISECOND));
	}
	release_reference(desktop);
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
		fputs("Could not initialize the fixture AT-SPI registry (atspi_init).\n", stderr);
		return EXIT_FAILURE;
	}
	const bool matched = find_control(&scan, strcmp(argv[2], "multi-line") == 0, argv[3]);
	const struct operation_description operation = describe_operation(observed_operation());
	enter_operation(matched ? DISCONNECT_AFTER_MATCH : DISCONNECT_AFTER_FAILURE);
	atspi_exit();
	alarm(0);
	if (!matched) {
		if (scan.rpc_error)
			fprintf(stderr, "AT-SPI query failed during %s: %.240s\n", operation.text, scan.rpc_error->message);
		else
			fprintf(stderr, "AT-SPI probe failed during %s: %s\n", operation.text, scan.failure);
		g_clear_error(&scan.rpc_error);
		return EXIT_FAILURE;
	}
	return EXIT_SUCCESS;
}
