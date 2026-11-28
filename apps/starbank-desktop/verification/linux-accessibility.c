#define _POSIX_C_SOURCE 200809L

#include <atspi/atspi-constants.h>
#include <gio/gio.h>
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
	MAX_DIAGNOSTIC_TEXT_BYTES = 160,
	PROBE_BUDGET_MS = 2500,
	PROBE_WATCHDOG_SECONDS = 3,
	RPC_TIMEOUT_MS = 400,
	RETRY_INTERVAL_MS = 50,
	STATE_WORD_BITS = 32,
	STATE_WORD_COUNT = 2,
};

struct object_reference {
	const char *destination;
	const char *path;
};

static const struct object_reference BUS_LAUNCHER = { "org.a11y.Bus", "/org/a11y/bus" };
static const struct object_reference REGISTRY_ROOT = {
	"org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root",
};
static const char WINDOW_TITLE[] = "StarBank Reloaded";
static const char GTK_OPEN_WINDOW_TITLE[] = "Open Bank";
static const char GTK_OPEN_LOCATION_OPTION[] = "--gtk-open-location";
static const char GTK_LOCATION_PLACEHOLDER[] = "Location";
static const char PLACEHOLDER_ATTRIBUTE[] = "placeholder-text";
static const char ACCESSIBLE_INTERFACE[] = "org.a11y.atspi.Accessible";
static const char TEXT_INTERFACE[] = "org.a11y.atspi.Text";
static const char NULL_PATH[] = "/org/a11y/atspi/null";

enum tree_scope {
	DESKTOP,
	DESKTOP_APPLICATION,
	APPLICATION_WINDOW,
	FRAME_CONTENT,
	WEB_CONTENT,
	GTK_CHOOSER_CONTENT,
};

enum probe_mode {
	WEBVIEW_ENTRY,
	GTK_OPEN_LOCATION,
};

struct scan {
	enum probe_mode mode;
	const char *control_name;
	GDBusConnection *bus;
	gint64 deadline;
	unsigned visited;
	bool window_found;
	bool document_found;
	GVariant *entry;
	GError *rpc_error;
	const char *failure;
};

#define PROBE_OPERATIONS(ENTRY) \
	ENTRY(CONNECT_SESSION, "connect to the private session bus") \
	ENTRY(GET_ADDRESS, "org.a11y.Bus.GetAddress") \
	ENTRY(CONNECT_ACCESSIBILITY, "connect to the private AT-SPI bus") \
	ENTRY(GET_ROLE, "Accessible.GetRole") \
	ENTRY(GET_NAME, "Accessible.Name") \
	ENTRY(GET_CHILDREN, "Accessible.GetChildren") \
	ENTRY(GET_STATES, "Accessible.GetState") \
	ENTRY(GET_ATTRIBUTES, "Accessible.GetAttributes") \
	ENTRY(GET_INTERFACES, "Accessible.GetInterfaces") \
	ENTRY(GET_CHARACTER_COUNT, "Text.CharacterCount") \
	ENTRY(GET_DIAGNOSTIC_TEXT, "Text.GetText after a CharacterCount mismatch") \
	ENTRY(GET_TEXT, "Text.GetText") \
	ENTRY(WAIT_FOR_REGISTRATION, "wait for registration or focus") \
	ENTRY(DISCONNECT_AFTER_MATCH, "GDBus cleanup after a confirmed match") \
	ENTRY(DISCONNECT_AFTER_FAILURE, "GDBus cleanup after a failed observation")

#define OPERATION_ENUM(name, text) name,
enum probe_operation { PROBE_OPERATIONS(OPERATION_ENUM) };
#undef OPERATION_ENUM

static atomic_int current_operation = CONNECT_SESSION;
_Static_assert(ATOMIC_INT_LOCK_FREE == 2, "The AT-SPI watchdog requires lock-free operation tracking.");

struct operation_description {
	const char *text;
	size_t length;
};

// Only selects string literals. The watchdog does not format or allocate text.
static struct operation_description describe_operation(enum probe_operation operation)
{
	switch (operation) {
#define OPERATION_CASE(name, text) case name: return (struct operation_description){ text, sizeof(text) - 1 };
	PROBE_OPERATIONS(OPERATION_CASE)
#undef OPERATION_CASE
	}
	return (struct operation_description){ "unknown operation", sizeof("unknown operation") - 1 };
}

#undef PROBE_OPERATIONS

static void enter_operation(enum probe_operation operation)
{
	atomic_store_explicit(&current_operation, operation, memory_order_relaxed);
}

static struct operation_description observed_operation(void)
{
	return describe_operation((enum probe_operation)atomic_load_explicit(&current_operation, memory_order_relaxed));
}

static void deadline_exceeded(int signal_number)
{
	(void)signal_number;
	const struct operation_description operation = observed_operation();
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

static gint next_timeout(struct scan *scan)
{
	if (scan->failure || scan->rpc_error)
		return 0;
	const gint64 remaining_ms = (scan->deadline - g_get_monotonic_time()) /
		G_TIME_SPAN_MILLISECOND;
	if (remaining_ms <= 0) {
		scan->failure = "The AT-SPI discovery deadline elapsed.";
		return 0;
	}
	return (gint)MIN(remaining_ms, RPC_TIMEOUT_MS);
}

static GVariant *query(struct scan *scan, GDBusConnection *bus, struct object_reference object,
	enum probe_operation operation, const char *interface, const char *method,
	GVariant *parameters, const GVariantType *reply_type)
{
	// Hold a non-floating reference even if the deadline prevents the call.
	if (parameters)
		g_variant_ref_sink(parameters);
	GVariant *reply = NULL;
	const gint timeout = next_timeout(scan);
	if (timeout > 0) {
		// Bus creation and Registry activation are separate. Let the fixed
		// services activate, while querying only already registered children.
		const bool registry_root = operation == GET_CHILDREN &&
			strcmp(object.destination, REGISTRY_ROOT.destination) == 0 &&
			strcmp(object.path, REGISTRY_ROOT.path) == 0;
		const GDBusCallFlags flags = operation == GET_ADDRESS || registry_root ?
			G_DBUS_CALL_FLAGS_NONE : G_DBUS_CALL_FLAGS_NO_AUTO_START;
		enter_operation(operation);
		reply = g_dbus_connection_call_sync(bus, object.destination, object.path, interface,
			method, parameters, reply_type, flags, timeout, NULL, &scan->rpc_error);
	}
	g_clear_pointer(&parameters, g_variant_unref);
	return reply;
}

static GVariant *property(struct scan *scan, struct object_reference object,
	enum probe_operation operation, const char *interface, const char *name,
	const GVariantType *type)
{
	GVariant *reply = query(scan, scan->bus, object, operation,
		"org.freedesktop.DBus.Properties", "Get", g_variant_new("(ss)", interface, name),
		G_VARIANT_TYPE("(v)"));
	if (!reply)
		return NULL;
	GVariant *value = NULL;
	g_variant_get(reply, "(v)", &value);
	g_variant_unref(reply);
	if (!g_variant_is_of_type(value, type)) {
		scan->failure = "The AT-SPI property returned an unexpected D-Bus type.";
		g_variant_unref(value);
		return NULL;
	}
	return value;
}

static bool name_matches(struct scan *scan, struct object_reference object, const char *name)
{
	GVariant *value = property(scan, object, GET_NAME, ACCESSIBLE_INTERFACE, "Name", G_VARIANT_TYPE_STRING);
	if (!value)
		return false;
	const bool matches = strcmp(g_variant_get_string(value, NULL), name) == 0;
	g_variant_unref(value);
	return matches;
}

static struct object_reference read_reference(GVariant *reference)
{
	struct object_reference object;
	g_variant_get(reference, "(&s&o)", &object.destination, &object.path);
	return object;
}

static bool gtk_location_entry(struct scan *scan, struct object_reference object)
{
	GVariant *reply = query(scan, scan->bus, object, GET_ATTRIBUTES, ACCESSIBLE_INTERFACE,
		"GetAttributes", NULL, G_VARIANT_TYPE("(a{ss})"));
	if (!reply)
		return false;
	GVariant *attributes = g_variant_get_child_value(reply, 0);
	g_variant_unref(reply);
	const char *placeholder;
	bool matches = false;
	if (g_variant_n_children(attributes) > MAX_VISITED_NODES)
		scan->failure = "The GTK entry returned too many AT-SPI attributes.";
	else if (g_variant_lookup(attributes, PLACEHOLDER_ATTRIBUTE, "&s", &placeholder))
		matches = strcmp(placeholder, GTK_LOCATION_PLACEHOLDER) == 0;
	g_variant_unref(attributes);
	return matches;
}

static void visit(struct scan *scan, GVariant *reference, enum tree_scope scope, unsigned depth)
{
	if (depth > MAX_TREE_DEPTH || scan->visited >= MAX_VISITED_NODES) {
		scan->failure = "The scoped AT-SPI tree exceeded the fixture limits.";
		return;
	}
	scan->visited++;
	const struct object_reference object = read_reference(reference);
	if (strcmp(object.path, NULL_PATH) == 0)
		return;
	if (!g_dbus_is_name(object.destination)) {
		scan->failure = "The AT-SPI child reference did not contain a D-Bus name.";
		return;
	}

	guint32 role = ATSPI_ROLE_DESKTOP_FRAME;
	if (scope != DESKTOP) {
		GVariant *reply = query(scan, scan->bus, object, GET_ROLE, ACCESSIBLE_INTERFACE,
			"GetRole", NULL, G_VARIANT_TYPE("(u)"));
		if (!reply)
			return;
		g_variant_get(reply, "(u)", &role);
		g_variant_unref(reply);
	}

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
		if (scan->mode == GTK_OPEN_LOCATION) {
			if (role != ATSPI_ROLE_FILE_CHOOSER || !name_matches(scan, object, GTK_OPEN_WINDOW_TITLE))
				return;
			children = GTK_CHOOSER_CONTENT;
		} else {
			if (role != ATSPI_ROLE_FRAME || !name_matches(scan, object, WINDOW_TITLE))
				return;
			children = FRAME_CONTENT;
		}
		if (scan->window_found) {
			scan->failure = scan->mode == GTK_OPEN_LOCATION ?
				"The fixture has more than one Open Bank AT-SPI file chooser." :
				"The fixture has more than one matching AT-SPI frame.";
			return;
		}
		scan->window_found = true;
		break;
	case FRAME_CONTENT:
		if (role == ATSPI_ROLE_DOCUMENT_WEB) {
			scan->document_found = true;
			children = WEB_CONTENT;
		}
		break;
	case WEB_CONTENT:
		if (role == ATSPI_ROLE_ENTRY && name_matches(scan, object, scan->control_name)) {
			if (scan->entry) {
				scan->failure = "More than one matching entry was exposed by the WebView.";
				return;
			}
			scan->entry = g_variant_ref(reference);
		}
		break;
	case GTK_CHOOSER_CONTENT:
		// GTK exposes the visible GtkEntry as TEXT and its Location placeholder as an attribute.
		if (role == ATSPI_ROLE_TEXT) {
			if (gtk_location_entry(scan, object)) {
				if (scan->entry)
					scan->failure = "The Open Bank chooser exposed more than one AT-SPI location entry.";
				else
					scan->entry = g_variant_ref(reference);
			}
			return;
		}
		// The location entry is in the header. Do not enumerate file or sidebar rows.
		if (role == ATSPI_ROLE_SCROLL_PANE || role == ATSPI_ROLE_TABLE || role == ATSPI_ROLE_TREE_TABLE ||
			role == ATSPI_ROLE_TREE || role == ATSPI_ROLE_LIST || role == ATSPI_ROLE_LIST_BOX ||
			role == ATSPI_ROLE_DOCUMENT_WEB)
			return;
		break;
	}

	GVariant *reply = query(scan, scan->bus, object, GET_CHILDREN, ACCESSIBLE_INTERFACE,
		"GetChildren", NULL, G_VARIANT_TYPE("(a(so))"));
	if (!reply)
		return;
	GVariant *list = g_variant_get_child_value(reply, 0);
	g_variant_unref(reply);
	const gsize count = g_variant_n_children(list);
	if (count > MAX_VISITED_NODES - scan->visited) {
		scan->failure = "The scoped AT-SPI child list exceeded the fixture limits.";
	} else {
		for (gsize index = 0; index < count && next_timeout(scan) > 0; index++) {
			GVariant *child = g_variant_get_child_value(list, index);
			visit(scan, child, children, depth + 1);
			g_variant_unref(child);
		}
	}
	g_variant_unref(list);
}

static bool exposes_text(struct scan *scan, struct object_reference object)
{
	GVariant *reply = query(scan, scan->bus, object, GET_INTERFACES, ACCESSIBLE_INTERFACE,
		"GetInterfaces", NULL, G_VARIANT_TYPE("(as)"));
	if (!reply)
		return false;
	GVariant *interfaces = g_variant_get_child_value(reply, 0);
	g_variant_unref(reply);
	bool found = false;
	if (g_variant_n_children(interfaces) > MAX_VISITED_NODES) {
		scan->failure = "The entry returned too many AT-SPI interfaces.";
	} else {
		GVariantIter iter;
		g_variant_iter_init(&iter, interfaces);
		const char *name;
		while (g_variant_iter_next(&iter, "&s", &name)) {
			if (strcmp(name, TEXT_INTERFACE) == 0) {
				found = true;
				break;
			}
		}
		if (!found)
			scan->failure = "The entry does not expose the AT-SPI Text interface.";
	}
	g_variant_unref(interfaces);
	return found;
}

static void report_count_mismatch(struct scan *scan, struct object_reference object,
	gint32 count, glong expected)
{
	fprintf(stderr, "AT-SPI CharacterCount mismatch (reported %" G_GINT32_FORMAT ", expected %ld).\n",
		count, expected);
	if (count < 0 || count > MAX_EXPECTED_TEXT_BYTES)
		return;

	// A failed diagnostic must not replace the original CharacterCount failure.
	struct scan diagnostic = { .deadline = scan->deadline };
	GVariant *reply = query(&diagnostic, scan->bus, object, GET_DIAGNOSTIC_TEXT, TEXT_INTERFACE,
		"GetText", g_variant_new("(ii)", 0, count), G_VARIANT_TYPE("(s)"));
	if (reply) {
		const char *actual;
		g_variant_get(reply, "(&s)", &actual);
		const bool truncated = strnlen(actual, MAX_DIAGNOSTIC_TEXT_BYTES + 1) > MAX_DIAGNOSTIC_TEXT_BYTES;
		char *bounded = g_strndup(actual, MAX_DIAGNOSTIC_TEXT_BYTES);
		// GLib escapes each byte to at most four ASCII characters.
		char *escaped = g_strescape(bounded, NULL);
		fprintf(stderr, "AT-SPI mismatch text: \"%s\"%s\n", escaped, truncated ? " (truncated)" : "");
		g_free(escaped);
		g_free(bounded);
		g_variant_unref(reply);
	} else {
		fputs("The bounded AT-SPI mismatch text read failed.\n", stderr);
	}
	g_clear_error(&diagnostic.rpc_error);
	enter_operation(GET_CHARACTER_COUNT);
}

static bool verify_text(struct scan *scan, struct object_reference object, const char *expected)
{
	if (!exposes_text(scan, object))
		return false;
	GVariant *property_value = property(scan, object, GET_CHARACTER_COUNT, TEXT_INTERFACE,
		"CharacterCount", G_VARIANT_TYPE_INT32);
	if (!property_value)
		return false;
	const gint32 count = g_variant_get_int32(property_value);
	g_variant_unref(property_value);
	const glong expected_count = g_utf8_strlen(expected, -1);
	if (count != expected_count) {
		scan->failure = "The AT-SPI text character count did not match the fixture.";
		report_count_mismatch(scan, object, count, expected_count);
		return false;
	}
	GVariant *reply = query(scan, scan->bus, object, GET_TEXT, TEXT_INTERFACE, "GetText",
		g_variant_new("(ii)", 0, count), G_VARIANT_TYPE("(s)"));
	if (!reply)
		return false;
	const char *actual;
	g_variant_get(reply, "(&s)", &actual);
	const bool matched = next_timeout(scan) > 0 && strcmp(actual, expected) == 0;
	g_variant_unref(reply);
	if (!matched && !scan->failure)
		scan->failure = "The AT-SPI text did not match the exact fixture text.";
	return matched;
}

static bool has_state(guint64 bits, AtspiStateType state)
{
	return (bits & ((guint64)1 << state)) != 0;
}

static bool verify_focused_entry(struct scan *scan, bool multiline, const char *expected)
{
	const struct object_reference object = read_reference(scan->entry);
	GVariant *reply = query(scan, scan->bus, object, GET_STATES, ACCESSIBLE_INTERFACE,
		"GetState", NULL, G_VARIANT_TYPE("(au)"));
	if (!reply)
		return false;
	GVariant *states = g_variant_get_child_value(reply, 0);
	g_variant_unref(reply);
	guint32 low, high;
	if (g_variant_n_children(states) != STATE_WORD_COUNT) {
		scan->failure = "The entry did not return two AT-SPI state words.";
		g_variant_unref(states);
		return false;
	}
	g_variant_get_child(states, 0, "u", &low);
	g_variant_get_child(states, 1, "u", &high);
	g_variant_unref(states);
	const guint64 bits = ((guint64)high << STATE_WORD_BITS) | low;
	if (!has_state(bits, ATSPI_STATE_FOCUSED))
		return false;
	const AtspiStateType line = multiline ? ATSPI_STATE_MULTI_LINE : ATSPI_STATE_SINGLE_LINE;
	const AtspiStateType other_line = multiline ? ATSPI_STATE_SINGLE_LINE : ATSPI_STATE_MULTI_LINE;
	if (!has_state(bits, ATSPI_STATE_FOCUSABLE) || !has_state(bits, ATSPI_STATE_EDITABLE) ||
		!has_state(bits, ATSPI_STATE_ENABLED) || !has_state(bits, ATSPI_STATE_SENSITIVE) ||
		!has_state(bits, line) || has_state(bits, other_line)) {
		scan->failure = "The focused entry did not have the expected AT-SPI states.";
		return false;
	}
	// GTK can focus the entry before its revealer maps it. Keep polling within the same deadline.
	if (scan->mode == GTK_OPEN_LOCATION &&
		(!has_state(bits, ATSPI_STATE_VISIBLE) || !has_state(bits, ATSPI_STATE_SHOWING)))
		return false;
	return verify_text(scan, object, expected);
}

static bool report_gtk_location_name(struct scan *scan)
{
	const struct object_reference object = read_reference(scan->entry);
	GVariant *value = property(scan, object, GET_NAME, ACCESSIBLE_INTERFACE, "Name", G_VARIANT_TYPE_STRING);
	if (!value)
		return false;
	gsize length;
	const char *name = g_variant_get_string(value, &length);
	char *bounded = g_strndup(name, MAX_CONTROL_NAME_BYTES);
	char *escaped = g_strescape(bounded, NULL);
	printf("GTK Open Bank location entry exposed AT-SPI role TEXT and name \"%s\"%s.\n",
		escaped, length > MAX_CONTROL_NAME_BYTES ? " (truncated)" : "");
	g_free(escaped);
	g_free(bounded);
	g_variant_unref(value);
	return next_timeout(scan) > 0;
}

static bool find_control(struct scan *scan, bool multiline, const char *expected)
{
	const char *pending = "The fixture AT-SPI desktop was unavailable.";
	GVariant *root = g_variant_ref_sink(g_variant_new("(so)", REGISTRY_ROOT.destination, REGISTRY_ROOT.path));
	bool matched = false;
	while (next_timeout(scan) > 0) {
		scan->visited = 0;
		scan->window_found = false;
		scan->document_found = false;
		visit(scan, root, DESKTOP, 0);
		if (scan->entry)
			matched = verify_focused_entry(scan, multiline, expected);
		if (scan->mode == GTK_OPEN_LOCATION) {
			if (matched)
				matched = report_gtk_location_name(scan);
			pending = !scan->window_found ? "The Open Bank AT-SPI file chooser was not exposed." :
				!scan->entry ? "The Open Bank AT-SPI location entry was not exposed." :
				"The Open Bank AT-SPI location entry was not focused, visible and showing.";
		} else {
			pending = !scan->window_found ? "The fixture AT-SPI frame was not exposed." :
				!scan->document_found ? "The fixture WebView document was not exposed." :
				!scan->entry ? "The named AT-SPI entry was not exposed." :
				"The named AT-SPI entry was not focused.";
		}
		g_clear_pointer(&scan->entry, g_variant_unref);
		if (matched || scan->failure || scan->rpc_error)
			break;
		const gint64 remaining = scan->deadline - g_get_monotonic_time();
		if (remaining < G_TIME_SPAN_MILLISECOND)
			break;
		enter_operation(WAIT_FOR_REGISTRATION);
		g_usleep((gulong)MIN(remaining, RETRY_INTERVAL_MS * G_TIME_SPAN_MILLISECOND));
	}
	g_variant_unref(root);
	if (!matched && !scan->failure && !scan->rpc_error)
		scan->failure = pending;
	return matched;
}

static GDBusConnection *connect_bus(struct scan *scan, enum probe_operation operation, const char *address)
{
	if (next_timeout(scan) == 0)
		return NULL;
	enter_operation(operation);
	if (!g_str_has_prefix(address, "unix:") || strchr(address, ';') || !g_dbus_is_address(address)) {
		scan->failure = "Expected a single Unix address for the fixture D-Bus connection.";
		return NULL;
	}
	// The process watchdog also bounds the constructor's connection handshake.
	return g_dbus_connection_new_for_address_sync(address,
		G_DBUS_CONNECTION_FLAGS_AUTHENTICATION_CLIENT | G_DBUS_CONNECTION_FLAGS_MESSAGE_BUS_CONNECTION,
		NULL, NULL, &scan->rpc_error);
}

static bool valid_text(const char *value, size_t maximum)
{
	const size_t length = strlen(value);
	return length > 0 && length <= maximum && g_utf8_validate(value, (gssize)length, NULL);
}

int main(int argc, char **argv)
{
	const bool gtk_open_location = argc > 1 && strcmp(argv[1], GTK_OPEN_LOCATION_OPTION) == 0;
	if (gtk_open_location) {
		if (argc != 3 || !valid_text(argv[2], MAX_EXPECTED_TEXT_BYTES)) {
			fputs("Expected --gtk-open-location and exact fixture text.\n", stderr);
			return EXIT_FAILURE;
		}
	} else if (argc != 4 || !valid_text(argv[1], MAX_CONTROL_NAME_BYTES) ||
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
	alarm(PROBE_WATCHDOG_SECONDS);
	struct scan scan = {
		.mode = gtk_open_location ? GTK_OPEN_LOCATION : WEBVIEW_ENTRY,
		.control_name = gtk_open_location ? NULL : argv[1],
		.deadline = g_get_monotonic_time() + PROBE_BUDGET_MS * G_TIME_SPAN_MILLISECOND,
	};
	GDBusConnection *session = connect_bus(&scan, CONNECT_SESSION, bus);
	if (session) {
		GVariant *reply = query(&scan, session, BUS_LAUNCHER, GET_ADDRESS, BUS_LAUNCHER.destination,
			"GetAddress", NULL, G_VARIANT_TYPE("(s)"));
		if (reply) {
			const char *address;
			g_variant_get(reply, "(&s)", &address);
			scan.bus = connect_bus(&scan, CONNECT_ACCESSIBILITY, address);
			g_variant_unref(reply);
		}
	}
	const bool multiline = !gtk_open_location && strcmp(argv[2], "multi-line") == 0;
	const char *expected = gtk_open_location ? argv[2] : argv[3];
	const bool matched = scan.bus && find_control(&scan, multiline, expected);
	const struct operation_description operation = observed_operation();
	enter_operation(matched ? DISCONNECT_AFTER_MATCH : DISCONNECT_AFTER_FAILURE);
	g_clear_object(&scan.bus);
	g_clear_object(&session);
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
