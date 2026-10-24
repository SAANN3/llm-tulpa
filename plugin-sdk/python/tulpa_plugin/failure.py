"""Why a call failed. The text goes back to whoever asked: the model for a tool, the owner for settings, the user for a tab
action, so say what went wrong in a way they can act on. Any other exception a hook raises is reported as an internal
failure with its type and message."""

# The protocol's error codes (plugin-sdk/PROTOCOL.md, "Errors")
BAD_ARGS = -32002
NOT_FOUND = -32001
INTERNAL = -32603


class Failure(Exception):
    """A failure that is the plugin's own (an internal one)"""

    code = INTERNAL


class BadArgs(Failure):
    """The arguments, or the settings, were wrong"""

    code = BAD_ARGS


class NotFound(Failure):
    """What the call asked for does not exist"""

    code = NOT_FOUND
