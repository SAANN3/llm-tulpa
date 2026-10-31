import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from tulpa_plugin import Block, Row, Section, Tab  # noqa: E402


class TabShape(unittest.TestCase):
    def test_a_tab_is_sent_in_the_shape_the_backend_reads(self):
        tab = (
            Tab()
            .section(
                Section("Hello")
                .block(Block.hint("a hint"))
                .block(Block.switch("loud", "Loud", True, help="why"))
                .block(Block.items("none", [Row("r1", "Row").subtitle("sub").button("greet", "Greet").upload_button("add", "Add").confirm_button("del", "Delete", "Sure?")]))
                .block(Block.drop_zone("upload", "Drop here", [".zip"]))
            )
            .message("done")
        ).to_json()
        self.assertEqual(tab["message"], "done")
        self.assertNotIn("popup", tab)
        blocks = tab["sections"][0]["blocks"]
        self.assertEqual(blocks[0], {"type": "text", "text": "a hint", "dim": True})
        self.assertEqual(blocks[1], {"type": "switch", "action": "loud", "label": "Loud", "on": True, "help": "why"})
        self.assertEqual(blocks[2]["rows"][0]["buttons"][1], {"action": "add", "label": "Add", "upload": True})
        self.assertEqual(blocks[2]["rows"][0]["buttons"][2]["confirm"], "Sure?")
        self.assertEqual(blocks[3], {"type": "drop", "action": "upload", "label": "Drop here", "accept": [".zip"]})

    def test_input_parameters_and_action_context_are_preserved(self):
        args = {"row": "server-1", "value": "old"}
        block = Block.input("save_url", "Server URL", "initial", placeholder="https://server/mcp",
                            help="Connection address", password=True, submit_label="Apply", args=args).to_json()
        args["row"] = "changed"
        self.assertEqual(block, {"type": "input", "action": "save_url", "label": "Server URL", "value": "initial",
                                 "placeholder": "https://server/mcp", "help": "Connection address", "password": True,
                                 "submit_label": "Apply", "args": {"row": "server-1", "value": "old"}})
        self.assertEqual(Block.input("save", "Name").to_json(),
                         {"type": "input", "action": "save", "label": "Name", "value": "", "password": False, "args": {}})

    def test_a_section_without_a_title_has_none_and_a_popup_is_kept(self):
        tab = Tab().section(Section()).popup("T", "text").to_json()
        self.assertNotIn("title", tab["sections"][0])
        self.assertEqual(tab["popup"], {"title": "T", "text": "text"})


if __name__ == "__main__":
    unittest.main()
