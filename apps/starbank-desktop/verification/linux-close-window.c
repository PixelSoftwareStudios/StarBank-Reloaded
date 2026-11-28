#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { WM_PROTOCOL_FORMAT = 32 };

static unsigned char x11_error_code;

static int record_x11_error(Display *display, XErrorEvent *error)
{
	(void)display;
	x11_error_code = error->error_code;
	return 0;
}

static Window parse_xid(const char *value)
{
	if (*value == '\0' || value[strspn(value, "0123456789")] != '\0')
		return None;

	errno = 0;
	const unsigned long xid = strtoul(value, NULL, 10);
	return errno == 0 && xid <= UINT32_MAX ? (Window)xid : None;
}

static bool advertises_delete(Display *display, Window window, Atom delete_atom)
{
	Atom *protocols = NULL;
	int count = 0;
	bool supported = false;
	if (XGetWMProtocols(display, window, &protocols, &count)) {
		for (int i = 0; i < count; ++i) {
			if (protocols[i] == delete_atom) {
				supported = true;
				break;
			}
		}
		XFree(protocols);
	}
	return supported;
}

int main(int argc, char **argv)
{
	const Window window = argc == 2 ? parse_xid(argv[1]) : None;
	if (window == None) {
		fputs("Expected one nonzero decimal X11 window ID.\n", stderr);
		return EXIT_FAILURE;
	}

	Display *display = XOpenDisplay(NULL);
	if (!display) {
		fputs("Could not open the X11 display.\n", stderr);
		return EXIT_FAILURE;
	}
	XSetErrorHandler(record_x11_error);

	const Atom protocols_atom = XInternAtom(display, "WM_PROTOCOLS", True);
	const Atom delete_atom = XInternAtom(display, "WM_DELETE_WINDOW", True);
	const bool supported = protocols_atom != None && delete_atom != None &&
		advertises_delete(display, window, delete_atom);
	Status sent = 0;
	if (supported) {
		// ICCCM 4.2.8 sends a close request directly to the client window.
		XEvent event = { .xclient = {
			.type = ClientMessage,
			.display = display,
			.window = window,
			.message_type = protocols_atom,
			.format = WM_PROTOCOL_FORMAT,
			.data.l = { (long)delete_atom, CurrentTime },
		} };
		sent = XSendEvent(display, window, False, NoEventMask, &event);
		XSync(display, False);
	}
	XCloseDisplay(display);

	if (x11_error_code != 0)
		fprintf(stderr, "X11 close request failed with error %u.\n", (unsigned)x11_error_code);
	else if (!supported)
		fputs("The X11 window does not advertise WM_DELETE_WINDOW.\n", stderr);
	else if (!sent)
		fputs("Could not send WM_DELETE_WINDOW.\n", stderr);
	else
		return EXIT_SUCCESS;
	return EXIT_FAILURE;
}
