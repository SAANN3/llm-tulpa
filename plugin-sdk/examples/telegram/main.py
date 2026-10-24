"""The built-in Telegram plugin as an external one: the bot is long-polled with the standard library's own HTTP client, and what
people write to it is answered by the plugin's agent (`host.agent_reply`). It shows what a plugin that brings messages in from
outside needs: a loop that runs while the plugin is on, settings with a secret, and the agent."""

import asyncio
import base64
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

# A plugin that is shipped carries its SDK in `tulpa_plugin/` next to this file (`python3 ../../python/vendor.py .` puts it
# there). In a checkout of llm-tulpa the SDK is two folders up, and is found there when no copy is next to this file.
sys.path.append(str(Path(__file__).resolve().parent.parent.parent / "python"))

from tulpa_plugin import BadArgs, Host, Plugin, run  # noqa: E402

DEFAULT_API_URL = "https://api.telegram.org"
#: How long Telegram holds a `getUpdates` request open when nothing has come; the HTTP timeout is longer, or every idle poll would
#: fail before Telegram answers
LONG_POLL_SECONDS = 30
#: Telegram refuses a message over 4096 characters
MAX_MESSAGE_CHARS = 4000
#: The longest wait between tries after Telegram could not be reached
MAX_BACKOFF_SECONDS = 30
MAX_PHOTO_BYTES = 10 * 1024 * 1024
#: An update offset written this recently is where a restarted plugin picks up, so what arrived while it was down is not lost;
#: an older one is a plugin that was off, and what piled up then is not answered
RESUME_WITHIN_SECONDS = 300
RECEIVED_REACTION = "\N{EYES}"
BARE_COMMAND_PROMPT = "What would you like to ask? (Reply to this message)"


class ApiError(Exception):
    def __init__(self, status: int, description: str):
        super().__init__(f"Telegram answered {status}: {description}")
        self.status = status


class Telegram:
    """The few calls of Telegram's Bot API the plugin makes. Blocking: it is called from threads."""

    def __init__(self, token: str, api_url: str):
        self.base = f"{api_url.rstrip('/')}/bot{token}"
        self.file_base = f"{api_url.rstrip('/')}/file/bot{token}"

    def call(self, method: str, params: dict | None = None, timeout: float = 15) -> object:
        request = urllib.request.Request(f"{self.base}/{method}", data=json.dumps(params or {}).encode(), headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                body = json.load(response)
        except urllib.error.HTTPError as error:
            try:
                description = json.load(error).get("description", error.reason)
            except ValueError:
                description = error.reason
            raise ApiError(error.code, str(description)) from None
        if not body.get("ok"):
            raise ApiError(int(body.get("error_code", 0)), str(body.get("description", "no reason given")))
        return body["result"]

    def download(self, file_path: str) -> bytes:
        with urllib.request.urlopen(f"{self.file_base}/{urllib.parse.quote(file_path)}", timeout=30) as response:
            return response.read(MAX_PHOTO_BYTES + 1)


def split_command(text: str) -> tuple[bool, str]:
    """Splits Telegram's own `/command` or `/command@botname` off the front of a message: `(True, "")` for a bare command,
    `(True, "the rest")` for one with a message after it, `(False, text)` for anything that is not a command. The model should
    never see the command syntax, only what was said."""
    if not text.startswith("/"):
        return False, text
    _, _, rest = text.partition(" ")
    return True, rest.strip() if " " in text else ""


def chunks(text: str, limit: int = MAX_MESSAGE_CHARS) -> list[str]:
    """The text in pieces Telegram takes, cut at a blank line when it can be and anywhere when it cannot"""
    pieces: list[str] = []
    current = ""
    for paragraph in text.split("\n\n"):
        while len(paragraph) > limit:
            if current:
                pieces.append(current)
                current = ""
            pieces.append(paragraph[:limit])
            paragraph = paragraph[limit:]
        if current and len(current) + 2 + len(paragraph) > limit:
            pieces.append(current)
            current = ""
        current = f"{current}\n\n{paragraph}" if current else paragraph
    if current:
        pieces.append(current)
    return pieces or [""]


class TelegramPlugin(Plugin):
    def __init__(self) -> None:
        self.host: Host | None = None
        self.api: Telegram | None = None
        self.allowed: set[str] = set()
        self.think = False
        self.name_hint = False
        self.timezone = "UTC"
        self.task: asyncio.Task | None = None

    async def configure(self, settings: dict, host: Host) -> None:
        self.host = host
        token = str(settings.get("token") or "").strip()
        if not token:
            raise BadArgs("the bot token is needed")
        self.api = Telegram(token, str(settings.get("api_url") or "").strip() or DEFAULT_API_URL)
        self.allowed = {str(chat) for chat in settings.get("allowed_chat_ids") or []}
        self.think = bool(settings.get("think"))
        self.name_hint = bool(settings.get("name_hint"))
        self.timezone = str(settings.get("timezone") or "").strip() or "UTC"
        # Telegram saying the token is wrong is the owner's to fix; Telegram being out of reach is not a reason to refuse it
        try:
            me = await asyncio.to_thread(self.api.call, "getMe")
            await host.log(f"the bot is @{me.get('username', '?')}")
        except ApiError as error:
            if error.status in (401, 404):
                raise BadArgs("Telegram does not accept this bot token") from None
            await host.log(f"could not check the token: {error}", "warn")
        except OSError as error:
            await host.log(f"could not reach Telegram to check the token: {error}", "warn")

    async def enable(self) -> None:
        self.task = asyncio.create_task(self.poll())

    async def disable(self) -> None:
        if self.task:
            self.task.cancel()
            self.task = None

    # ---- the offset: where the next poll resumes

    def offset_file(self) -> Path | None:
        directory = self.host.data_dir if self.host else None
        return directory / "offset.json" if directory else None

    def load_offset(self) -> int | None:
        path = self.offset_file()
        try:
            if path and time.time() - path.stat().st_mtime < RESUME_WITHIN_SECONDS:
                return int(json.loads(path.read_text())["offset"])
        except (OSError, ValueError, KeyError):
            pass
        return None

    def save_offset(self, offset: int) -> None:
        path = self.offset_file()
        if path:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps({"offset": offset}))

    # ---- the loop

    async def poll(self) -> None:
        assert self.api and self.host
        api, host = self.api, self.host
        backoff = 2
        offset = self.load_offset()
        connected = False
        while True:
            try:
                if offset is None:
                    # What piled up while nothing was listening is not answered: start after the newest update
                    newest = await asyncio.to_thread(api.call, "getUpdates", {"offset": -1, "timeout": 0})
                    offset = newest[-1]["update_id"] + 1 if newest else 0
                    await asyncio.to_thread(api.call, "setMyCommands", {"commands": [{"command": "chat", "description": "Talk to the assistant: type your message after the command"}]})
                updates = await asyncio.to_thread(api.call, "getUpdates", {"offset": offset, "timeout": LONG_POLL_SECONDS}, LONG_POLL_SECONDS + 15)
                if not connected:
                    await host.log("connected to Telegram, waiting for messages")
                    connected = True
                backoff = 2
                for update in updates:
                    offset = update["update_id"] + 1
                    try:
                        await self.on_update(update)
                    except Exception as error:  # noqa: BLE001 -- one bad message must not stop the bot
                        await host.log(f"could not answer an update: {type(error).__name__}: {error}", "error")
                    self.save_offset(offset)
            except asyncio.CancelledError:
                raise
            except (ApiError, OSError, ValueError) as error:
                connected = False
                await host.log(f"Telegram could not be reached ({error}); trying again in {backoff} s", "warn")
                await asyncio.sleep(backoff)
                backoff = min(backoff * 2, MAX_BACKOFF_SECONDS)

    # ---- one message

    async def on_update(self, update: dict) -> None:
        assert self.api and self.host
        message = update.get("message")
        if not message:
            return
        text = message.get("text") or message.get("caption") or ""
        photos = message.get("photo")
        if not text and not photos:
            return
        chat_id = str(message["chat"]["id"])
        sender = message.get("from") or {}
        author = " ".join(part for part in (sender.get("first_name"), sender.get("last_name")) if part) or "unknown"
        if chat_id not in self.allowed:
            # The only way to learn a chat's id is to see it here, so it is written down
            await self.host.log(f"ignoring chat {chat_id} ({author}): it is not in allowed_chat_ids")
            return

        is_command, content = split_command(text)
        if is_command and not content and not photos:
            await self.send(chat_id, BARE_COMMAND_PROMPT, message["message_id"], force_reply=True)
            return
        content = content if is_command else text

        await self.react(chat_id, message["message_id"])
        images = []
        if photos:
            try:
                images.append(await self.download_photo(photos))
            except (ApiError, OSError, ValueError) as error:
                await self.host.log(f"could not download a photo in chat {chat_id}: {error}", "warn")
        if self.name_hint:
            content = f"[Message from user named {author} with userid {sender.get('id', 'unknown')}, sent at {self.sent_at(message.get('date'))}]\n{content}"
        if not content.strip() and not images:
            return

        answer = await self.host.agent_reply(chat_id, content, title=author, images=images, think=self.think)
        for at, piece in enumerate(chunks(answer)):
            await self.send(chat_id, piece, message["message_id"] if at == 0 else None)
        await self.host.log(f"answered chat {chat_id}")

    def sent_at(self, when: int | None) -> str:
        try:
            from zoneinfo import ZoneInfo

            zone = ZoneInfo(self.timezone)
        except Exception:  # noqa: BLE001 -- an unknown name, or no tz database on this machine
            zone = timezone.utc
        return datetime.fromtimestamp(when or time.time(), zone).strftime("%Y-%m-%d %H:%M:%S %Z")

    async def react(self, chat_id: str, message_id: int) -> None:
        """An immediate "seen, working on it"; a missing reaction is not worth failing the answer for"""
        assert self.api and self.host
        try:
            await asyncio.to_thread(self.api.call, "setMessageReaction", {"chat_id": chat_id, "message_id": message_id, "reaction": [{"type": "emoji", "emoji": RECEIVED_REACTION}]})
        except (ApiError, OSError) as error:
            await self.host.log(f"could not react in chat {chat_id}: {error}", "warn")

    async def send(self, chat_id: str, text: str, reply_to: int | None, force_reply: bool = False) -> None:
        assert self.api
        params: dict = {"chat_id": chat_id, "text": text}
        if reply_to is not None:
            params["reply_parameters"] = {"message_id": reply_to, "allow_sending_without_reply": True}
        if force_reply:
            # Opens the reply box on the sender's own client, so their next message arrives as a reply to this prompt
            params["reply_markup"] = {"force_reply": True, "selective": True, "input_field_placeholder": "Type your message..."}
        await asyncio.to_thread(self.api.call, "sendMessage", params)

    async def download_photo(self, photos: list) -> str:
        """The biggest size of a photo, as base64"""
        assert self.api
        best = max(photos, key=lambda p: p.get("file_size") or p.get("width", 0) * p.get("height", 0))
        info = await asyncio.to_thread(self.api.call, "getFile", {"file_id": best["file_id"]})
        data = await asyncio.to_thread(self.api.download, info["file_path"])
        if len(data) > MAX_PHOTO_BYTES:
            raise ValueError("the photo is larger than 10 MB")
        return base64.b64encode(data).decode()


if __name__ == "__main__":
    run(TelegramPlugin())
