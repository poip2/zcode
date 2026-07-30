"""
zcode — Agent orchestration loop demo (Python skeleton).

Simulates the core Agent loop:
1. Receive user input
2. LLM decides tool call or text response
3. Execute tool
4. Loop until done

All external deps (LLM API) are stubbed.
Runs with stdlib only.
"""

import json
import time
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional


# ── Message Types ──────────────────────────────────────────────

class StopReason(Enum):
    END_TURN = "end_turn"
    TOOL_USE = "tool_use"
    MAX_TOKENS = "max_tokens"
    ERROR = "error"


@dataclass
class Usage:
    input_tokens: int = 0
    output_tokens: int = 0


@dataclass
class ToolCall:
    call_id: str
    tool_name: str
    arguments: dict


@dataclass
class AssistantMessage:
    content: list
    model: str
    usage: Usage
    stop_reason: StopReason
    tool_calls: list = field(default_factory=list)


@dataclass
class ToolResultMessage:
    tool_call_id: str
    tool_name: str
    content: str
    is_error: bool = False


@dataclass
class UserMessage:
    content: str


# ── Tool Definitions ───────────────────────────────────────────

TOOL_SPEC = {
    "read": {"description": "Read file contents", "parameters": {"path": "string"}},
    "write": {"description": "Write file contents", "parameters": {"path": "string", "content": "string"}},
    "edit": {"description": "Edit file by replacing text", "parameters": {"path": "string", "oldText": "string", "newText": "string"}},
    "shell": {"description": "Execute a shell command", "parameters": {"command": "string"}},
    "grep": {"description": "Search file contents", "parameters": {"pattern": "string", "path": "string"}},
    "find": {"description": "Find files by glob", "parameters": {"pattern": "string"}},
    "ls": {"description": "List directory contents", "parameters": {"path": "string"}},
}


def execute_tool(tool_name: str, args: dict) -> ToolResultMessage:
    """Execute a tool and return its result (stub)."""
    call_id = f"call_{time.time_ns()}"
    if tool_name == "read":
        content = f"# Fake file content for {args.get('path', '?')}\n\nThis is a stub."
    elif tool_name == "write":
        content = f"Written {len(args.get('content', ''))} bytes to {args.get('path', '?')}"
    elif tool_name == "edit":
        content = f"Replaced text in {args.get('path', '?')}"
    elif tool_name == "shell":
        content = f"$ {args.get('command', '')}\n> fake stdout output (stub)"
    elif tool_name == "grep":
        content = "file.md:3: matched line (stub)"
    elif tool_name == "find":
        content = json.dumps(["file1.md", "file2.md", "src/main.rs"])
    elif tool_name == "ls":
        content = json.dumps(["README.md", "src/", "docs/", "package.json"])
    else:
        return ToolResultMessage(call_id, tool_name, f"Unknown tool: {tool_name}", is_error=True)
    return ToolResultMessage(call_id, tool_name, content, is_error=False)


def stub_llm_stream(messages: list) -> list[AssistantMessage]:
    """
    Stub LLM that simulates multi-step tool calling.

    Turn 1: decides to read a file
    Turn 2: decides to edit it
    Turn 3: ends turn
    """
    turns = [
        AssistantMessage(
            content=["I'll read the file first."],
            model="stub-model",
            usage=Usage(input_tokens=100, output_tokens=20),
            stop_reason=StopReason.TOOL_USE,
            tool_calls=[ToolCall(call_id="call_1", tool_name="read", arguments={"path": "/tmp/test.md"})],
        ),
        AssistantMessage(
            content=["Now I'll update it."],
            model="stub-model",
            usage=Usage(input_tokens=130, output_tokens=15),
            stop_reason=StopReason.TOOL_USE,
            tool_calls=[ToolCall(call_id="call_2", tool_name="edit", arguments={
                "path": "/tmp/test.md",
                "oldText": "old content",
                "newText": "new content",
            })],
        ),
        AssistantMessage(
            content=["Done! Here's the file summary: updated the content."],
            model="stub-model",
            usage=Usage(input_tokens=200, output_tokens=30),
            stop_reason=StopReason.END_TURN,
        ),
    ]
    return turns


# ── Agent Loop ─────────────────────────────────────────────────

def agent_loop(user_input: str, session_label: str = "default"):
    """Simulate the core Agent::run() loop."""
    print(f"[{session_label}] === Agent Start ===")
    print(f"[{session_label}] User: {user_input}")
    print()

    messages: list = [UserMessage(content=user_input)]
    llm_turns = stub_llm_stream(messages)
    turn_index = 0

    for assistant_msg in llm_turns:
        turn_index += 1
        print(f"[{session_label}] === Turn {turn_index} ===")

        # Simulate streaming output
        for block in assistant_msg.content:
            print(f"[{session_label}] Assistant (streaming): {block}")
        print(f"[{session_label}]   Usage: in={assistant_msg.usage.input_tokens} out={assistant_msg.usage.output_tokens}")
        print(f"[{session_label}]   Stop reason: {assistant_msg.stop_reason.value}")

        messages.append(assistant_msg)

        if assistant_msg.stop_reason == StopReason.END_TURN:
            print(f"[{session_label}] === Agent End (normal) ===\n")
            return

        # Execute tool calls
        for tc in assistant_msg.tool_calls:
            print(f"[{session_label}]   → Tool: {tc.tool_name}({tc.arguments})")
            result = execute_tool(tc.tool_name, tc.arguments)
            print(f"[{session_label}]   ← Result: {result.content[:80]}...")
            messages.append(result)

        print()

    print(f"[{session_label}] === Agent End (max turns) ===\n")


# ── Demo Runner ────────────────────────────────────────────────

def main():
    print("=" * 50)
    print("zcode Agent Loop Demo (skeleton)")
    print("=" * 50)
    print()

    # Run a single agent turn sequence
    agent_loop("Update the README to mention the new feature.", "session-1")

    # Demonstrate other key components in text
    print("── File system commands ──")
    from pathlib import Path
    print(f"  resolve_path('/tmp')        → {Path('/tmp').resolve()}")
    print(f"  path_exists('/tmp')          → {Path('/tmp').exists()}")
    print(f"  list_folder_flat('/tmp')     → [dirs/files in /tmp]")

    print()
    print("── Workspace four-folder convention ──")
    print("  pin/       → markdown notes")
    print("  scripts/   → scripts written by AI")
    print("  sources/   → non-md files to edit")
    print("  output/    → generated artifacts")
    print()
    print("Done.")


if __name__ == "__main__":
    main()
