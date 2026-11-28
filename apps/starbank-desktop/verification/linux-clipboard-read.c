#define _POSIX_C_SOURCE 200809L

#include <X11/Xlib.h>
#include <errno.h>
#include <poll.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

// Only run on the fixture-owned Xvfb display after its synthetic Copy action.
// Xlib selection/property semantics: https://www.x.org/releases/current/doc/libX11/libX11/libX11.html
enum {
	READ_TIMEOUT_MS = 3000,
	MAX_CLIPBOARD_BYTES = 4096,
	PROPERTY_UNIT_BYTES = 4,
	TEXT_FORMAT = 8,
	MILLISECONDS_PER_SECOND = 1000,
	NANOSECONDS_PER_MILLISECOND = 1000000,
	NANOSECONDS_PER_SECOND = 1000000000,
};

static const char CLIPBOARD_SELECTION[] = "CLIPBOARD";
static const char UTF8_TARGET[] = "UTF8_STRING";
static const char PLAIN_UTF8_TARGET[] = "text/plain;charset=utf-8";
static const char PLAIN_UTF8_OPTION[] = "--plain-utf8";
static const char RESULT_PROPERTY[] = "_STARBANK_CLIPBOARD_READ";
static unsigned char x11_error_code;

static int record_x11_error(Display *display, XErrorEvent *error)
{
	(void)display;
	x11_error_code = error->error_code;
	return 0;
}

static int64_t monotonic_milliseconds(void)
{
	struct timespec now;
	if (clock_gettime(CLOCK_MONOTONIC, &now) != 0 || now.tv_sec < 0 ||
		now.tv_nsec < 0 || now.tv_nsec >= NANOSECONDS_PER_SECOND ||
		(uint64_t)now.tv_sec >
			(uint64_t)(INT64_MAX - MILLISECONDS_PER_SECOND) / MILLISECONDS_PER_SECOND)
		return -1;
	return (int64_t)now.tv_sec * MILLISECONDS_PER_SECOND +
		now.tv_nsec / NANOSECONDS_PER_MILLISECOND;
}

static const char *wait_for_selection(
	Display *display, Window requestor, Atom selection, Atom target, Atom property)
{
	const int64_t started = monotonic_milliseconds();
	if (started < 0 || started > INT64_MAX - READ_TIMEOUT_MS)
		return "Could not measure the clipboard timeout.";
	const int64_t deadline = started + READ_TIMEOUT_MS;
	struct pollfd connection = { .fd = ConnectionNumber(display), .events = POLLIN };

	for (;;) {
		const int64_t now = monotonic_milliseconds();
		if (now < started)
			return "Could not measure the clipboard timeout.";
		if (now >= deadline)
			return "Timed out waiting for the clipboard selection.";
		if (x11_error_code != 0)
			return "The X11 clipboard request failed.";
		if (XPending(display) > 0) {
			XEvent event;
			XNextEvent(display, &event);
			if (event.type != SelectionNotify)
				continue;
			const XSelectionEvent *notification = &event.xselection;
			// ConvertSelection preserves CurrentTime; GTK 3.24.41 echoes the request time.
			if (notification->requestor != requestor || notification->selection != selection ||
				notification->target != target || notification->time != CurrentTime)
				return "The clipboard selection reply did not match the request.";
			if (notification->property == None)
				return "The clipboard owner refused the requested text target.";
			if (notification->property != property)
				return "The clipboard owner returned an unexpected property.";
			return NULL;
		}

		const int ready = poll(&connection, 1, (int)(deadline - now));
		if (ready < 0 && errno == EINTR)
			continue;
		if (ready < 0 || (connection.revents & (POLLERR | POLLHUP | POLLNVAL)) != 0)
			return "The X11 clipboard connection failed.";
	}
}

int main(int argc, char **argv)
{
	const char *target_name = UTF8_TARGET;
	if (argc == 2 && strcmp(argv[1], PLAIN_UTF8_OPTION) == 0) {
		target_name = PLAIN_UTF8_TARGET;
	} else if (argc != 1) {
		fprintf(stderr, "The clipboard fixture reader accepts only the optional %s flag.\n",
			PLAIN_UTF8_OPTION);
		return EXIT_FAILURE;
	}
	const char *display_name = getenv("DISPLAY");
	if (!display_name || *display_name == '\0') {
		fputs("The clipboard fixture requires its Xvfb DISPLAY.\n", stderr);
		return EXIT_FAILURE;
	}
	Display *display = XOpenDisplay(display_name);
	if (!display) {
		fputs("Could not open the fixture X11 display.\n", stderr);
		return EXIT_FAILURE;
	}
	XSetErrorHandler(record_x11_error);

	Window requestor = None;
	unsigned char *bytes = NULL;
	unsigned long length = 0;
	const char *failure = NULL;

	const Atom selection = XInternAtom(display, CLIPBOARD_SELECTION, True);
	if (selection == None) {
		failure = "The fixture clipboard selection does not exist.";
		goto cleanup;
	}
	const Window owner = XGetSelectionOwner(display, selection);
	if (owner == None) {
		failure = "The fixture clipboard selection has no owner.";
		goto cleanup;
	}
	// A selection owner can register a GDK target before it has an X server atom.
	const Atom target = XInternAtom(display, target_name, False);
	if (target == None) {
		failure = "Could not intern the requested clipboard target.";
		goto cleanup;
	}
	const Atom property = XInternAtom(display, RESULT_PROPERTY, False);
	if (property == None) {
		failure = "Could not intern the clipboard reply property.";
		goto cleanup;
	}

	// The requestor stays unmapped and owns only the reply property.
	requestor = XCreateSimpleWindow(display, DefaultRootWindow(display), 0, 0, 1, 1, 0, 0, 0);
	XDeleteProperty(display, requestor, property);
	XConvertSelection(display, selection, target, property, requestor, CurrentTime);
	XFlush(display);
	failure = wait_for_selection(display, requestor, selection, target, property);
	if (failure)
		goto cleanup;

	Atom actual_type = None;
	int format = 0;
	unsigned long remaining = 0;
	const int status = XGetWindowProperty(display, requestor, property, 0,
		MAX_CLIPBOARD_BYTES / PROPERTY_UNIT_BYTES, False, target,
		&actual_type, &format, &length, &remaining, &bytes);
	if (status != Success || actual_type != target || format != TEXT_FORMAT ||
		length > MAX_CLIPBOARD_BYTES || remaining != 0 || (length > 0 && !bytes)) {
		failure = "The clipboard reply did not contain complete, bounded text for the requested target.";
		goto cleanup;
	}
	if (XGetSelectionOwner(display, selection) != owner) {
		failure = "The clipboard owner changed during readback.";
		goto cleanup;
	}
	XDeleteProperty(display, requestor, property);

cleanup:
	if (requestor != None)
		XDestroyWindow(display, requestor);
	XSync(display, False);
	XCloseDisplay(display);
	if (!failure && x11_error_code != 0)
		failure = "The X11 clipboard request failed.";
	if (!failure && ((length > 0 && fwrite(bytes, 1, (size_t)length, stdout) != length) ||
		fflush(stdout) != 0))
		failure = "Could not write the clipboard readback.";
	if (bytes)
		XFree(bytes);
	if (failure)
		fprintf(stderr, "%s (%s)\n", failure, target_name);
	return failure ? EXIT_FAILURE : EXIT_SUCCESS;
}
