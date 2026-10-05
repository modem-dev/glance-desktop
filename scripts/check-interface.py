#!/usr/bin/env python3
"""Check the built CLI and both MCP transports using synthetic local work only."""
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time

EDITOR_TOOLS = {
    "get_editor_state", "dispatch_action", "open_editor", "get_document",
    "import_image", "add_annotation", "update_annotation", "move_annotation",
    "delete_annotation", "crop_image", "resize_image", "set_backdrop",
    "undo", "redo", "read_image", "export_png", "export_mp4", "export_gif",
    "read_video_frame",
}
CODE_TOOLS = {"codemode_search", "codemode_execute", "codemode_execution",
              "codemode_decide", "codemode_cancel"}


class MCP:
    def __init__(self, argv, env):
        self.process = subprocess.Popen(argv, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        text=True, env=env)
        self.sequence = 0
        initialized = self.request("initialize", {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": "glance-interface-check", "version": "1"},
        })
        assert initialized["serverInfo"]["name"] == "glance", initialized
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def request(self, method, params):
        self.sequence += 1
        identifier = self.sequence
        self.send({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.process.stdout], [], [], max(0, deadline - time.monotonic()))
            if not ready:
                break
            line = self.process.stdout.readline()
            if not line:
                raise AssertionError("MCP disconnected: " + self.process.stderr.read())
            response = json.loads(line)
            if response.get("id") == identifier:
                assert "error" not in response, response
                return response["result"]
        raise AssertionError("MCP request timed out: " + method)

    def call(self, name, arguments):
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=5)
        assert self.process.returncode == 0, self.process.stderr.read()


def state(response):
    assert not response.get("isError"), response
    return response["structuredContent"]


def wait_state(client, identifier, expected):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        result = state(client.call("codemode_execution", {"id": identifier}))
        if result["status"] != "running":
            assert result["status"] == expected, result
            return result
        time.sleep(0.01)
    raise AssertionError("Execution did not reach " + expected)


def main():
    binary = str(Path(sys.argv[1]).resolve())
    assert Path(binary).is_file(), binary
    help_text = subprocess.check_output([binary, "--help"], text=True)
    assert "get-document" in help_text and "code" in help_text
    assert subprocess.check_output([binary], text=True) == help_text
    assert subprocess.check_output([binary, "--cli", "--help"], text=True) == help_text
    desktop_help = subprocess.check_output([binary, "desktop", "--help"], text=True)
    assert "Usage: glance desktop" in desktop_help
    assert "get-document" not in desktop_help
    command_help = subprocess.check_output([binary, "get-document", "--help"], text=True)
    assert command_help.startswith("glance get-document"), command_help

    with tempfile.TemporaryDirectory(prefix="glance-interface-") as directory:
        env = dict(os.environ, GLANCE_CODE_DIR=directory)
        direct = MCP([binary, "--mcp"], env)
        try:
            tools = direct.request("tools/list", {})["tools"]
            names = {tool["name"] for tool in tools}
            assert EDITOR_TOOLS <= names, EDITOR_TOOLS - names
            reads = {"get_editor_state", "get_document", "read_image", "read_video_frame"}
            for tool in tools:
                if tool["name"] in EDITOR_TOOLS:
                    read_only = tool["name"] in reads
                    expected = {"readOnlyHint": read_only, "destructiveHint": not read_only,
                                "idempotentHint": read_only, "openWorldHint": False}
                    for key, value in expected.items():
                        assert tool["annotations"][key] == value, tool
            assert {"code_search", "code_execute", "code_execution", "code_decide", "code_cancel"} <= names
            invalid = direct.call("read_image", {"max_edge": 1})
            assert invalid.get("isError"), invalid
            assert "Out of range: arguments.max_edge" in json.dumps(invalid), invalid
        finally:
            direct.close()

        server = subprocess.Popen([binary, "code", "serve"],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                  text=True, env=env)
        try:
            deadline = time.monotonic() + 10
            while not Path(directory, "service.sock").exists():
                assert server.poll() is None, server.stderr.read()
                assert time.monotonic() < deadline, "Code Mode service did not bind"
                time.sleep(0.01)
            second = subprocess.run([binary, "code", "serve"], env=env,
                                    text=True, capture_output=True, timeout=10)
            assert second.returncode != 0 and "already running" in second.stdout + second.stderr
            client = MCP([binary, "--codemode-mcp"], env)
            try:
                names = {tool["name"] for tool in client.request("tools/list", {})["tools"]}
                assert names == CODE_TOOLS, names
                found = state(client.call("codemode_search", {"query": "export_png"}))
                assert "export_png" in json.dumps(found), found
                started = state(client.call("codemode_execute", {"code": "return await Promise.resolve({answer:42});"}))
                result = wait_state(client, started["id"], "completed")
                assert result["result"] == {"answer": 42}, result

                # A second CLI client reads the same MCP-started execution.
                saved = json.loads(subprocess.check_output([
                    binary, "code", "execution", "--id", started["id"],
                    "--filter-output", "id,status,result"], env=env, text=True))
                assert saved["result"] == {"answer": 42}, saved

                mark = "{tool:'rectangle',points:[[1,2],[10,20]],color:[255,0,0,255],width:2,text:''}"
                paused = state(client.call("codemode_execute", {
                    "code": "return await glance.add_annotation({mark:" + mark + "});"}))
                paused = wait_state(client, paused["id"], "paused")
                sequence = next(entry["seq"] for entry in paused["log"] if entry["state"] == "pending")
                rejected = state(client.call("codemode_decide", {
                    "id": paused["id"], "seq": sequence, "decision": "reject"}))
                assert rejected["status"] == "rejected", rejected

                active = state(client.call("codemode_execute", {"code": "while (true) {}"}))
                time.sleep(0.05)
                cancelled = state(client.call("codemode_cancel", {"id": active["id"]}))
                assert cancelled["status"] == "cancelled", cancelled
                assert "get_document" in json.dumps(state(client.call("codemode_search", {"query": "get_document"})))
            finally:
                client.close()
        finally:
            server.terminate()
            server.wait(timeout=5)
        print(json.dumps({"editor_tools": 19, "code_mode_tools": 5,
                          "mcp_wire": "passed", "cross_client_history": "passed",
                          "cpu_loop_cancellation": "passed"}))


if __name__ == "__main__":
    main()
