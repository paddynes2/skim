"""No model or Google calls: exercise the boundaries before deploying the adapter."""
import asyncio
from email.message import EmailMessage
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import estate_reply as E

REQUEST = {"account": E.ACCOUNT, "messageId": "<original@example.com>", "instruction": "Suggest next week"}


def raw(mid="<original@example.com>", body="Question", reply=False):
    m = EmailMessage()
    m["Message-ID"] = mid
    m["From"] = "Client <client@example.com>" if not reply else E.ACCOUNT
    m["To"] = E.ACCOUNT if not reply else "client@example.com"
    m["Subject"] = "Project"
    if reply:
        m["In-Reply-To"] = REQUEST["messageId"]
    m.set_content(body)
    return "Subject: Project\n--- RAW MIME ---\n" + m.as_string()


class FakeGateway:
    def __init__(self, existing=False, changed=False, uncertain=False):
        self.existing, self.changed, self.uncertain = existing, changed, uncertain
        self.saves = 0
        self.reads = 0
        self.saved = None

    async def call(self, name, **args):
        if name == "search_gmail_messages":
            return "Found 1 messages matching 'query':\n  1. Message ID: abc\n     Web Link: url\n     Thread ID: def\n"
        if name == "get_gmail_message_content":
            return raw("<draft@example.com>", "Reply", True) if args["message_id"] == "123" else raw()
        if name == "get_gmail_thread_content":
            self.reads += 1
            return "original thread" + ("new reply" if self.changed and self.reads > 1 else "")
        if name == "draft_gmail_message":
            self.saves += 1
            self.saved = args
            if self.uncertain:
                raise TimeoutError("write may have succeeded")
            return "Draft created! Draft ID: r1"
        raise AssertionError(name)

    async def draft_messages(self, thread):
        return ["123"] if self.existing or self.saves else []


def candidate(*_):
    return {"body": "Reply", "sources": ["clients/example/current.md:10"], "gaps": []}


class Tests(unittest.TestCase):
    def test_identity_and_input_boundaries(self):
        key, clean = E.validate(REQUEST)
        self.assertEqual(len(key), 64)
        self.assertEqual(clean, REQUEST)
        for bad in [dict(REQUEST, account="other@example.com"), dict(REQUEST, messageId="x\nBcc: x"),
                    dict(REQUEST, messageId="<not-an-id>"), dict(REQUEST, instruction="x" * 2001)]:
            with self.assertRaises(E.Refused):
                E.validate(bad)

    def test_header_identity_and_recipient_are_host_owned(self):
        headers = E.target_headers(E.parse_raw(raw()), REQUEST["messageId"])
        self.assertEqual(headers["to"], "client@example.com")
        self.assertEqual(headers["subject"], "Re: Project")
        self.assertEqual(headers["references"], REQUEST["messageId"])
        with self.assertRaises(E.Refused):
            E.target_headers(E.parse_raw(raw()), "<different@example.com>")

    def test_search_contract_refuses_partial_parsing(self):
        self.assertEqual(E.parse_search("No messages found for query: 'x'"), [])
        with self.assertRaises(E.Refused):
            E.parse_search("Found 2 messages matching 'x':\n  1. Message ID: a\n Thread ID: b\n")

    def test_duplicate_start_and_uncertain_save_never_spawn_again(self):
        with tempfile.TemporaryDirectory() as root:
            launches = []
            E.start(REQUEST, Path(root), launches.append)
            E.start(REQUEST, Path(root), launches.append)
            self.assertEqual(len(launches), 1)
            E.receipt(Path(root) / launches[0], "uncertain")
            E.start(REQUEST, Path(root), launches.append)
            self.assertEqual(len(launches), 1)

    def run_prepare(self, gateway):
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root) / "request"
            directory.mkdir()
            result = asyncio.run(E.prepare(directory, REQUEST, gateway, candidate))
            self.assertEqual(result["status"], "ready")
            self.assertEqual(result["rfcMessageId"], "<draft@example.com>")

    def test_success_saves_once_and_verifies_exact_reply(self):
        g = FakeGateway()
        self.run_prepare(g)
        self.assertEqual(g.saves, 1)
        self.assertEqual(g.saved["thread_id"], "def")
        self.assertEqual(g.saved["to"], "client@example.com")
        self.assertFalse(g.saved["include_signature"])

    def test_existing_draft_prevents_generation(self):
        g = FakeGateway(existing=True)
        with self.assertRaisesRegex(E.Refused, "already has"):
            self.run_prepare(g)
        self.assertEqual(g.saves, 0)

    def test_new_mail_prevents_stale_draft_save(self):
        g = FakeGateway(changed=True)
        with self.assertRaisesRegex(E.Refused, "changed"):
            self.run_prepare(g)
        self.assertEqual(g.saves, 0)

    def test_unknown_save_outcome_is_durably_marked_before_call(self):
        g = FakeGateway(uncertain=True)
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root) / "request"
            directory.mkdir()
            with self.assertRaises(TimeoutError):
                asyncio.run(E.prepare(directory, REQUEST, g, candidate))
            self.assertEqual(E.read_receipt(directory)["status"], "saving")
            self.assertEqual(g.saves, 1)

    def test_agent_cannot_call_draft_creation(self):
        text = E.prompt_for(REQUEST, "abc", "def")
        self.assertIn("Do not write files, create drafts, send", text.replace("\n", " "))
        self.assertIn("untrusted data", text)


if __name__ == "__main__":
    unittest.main()
