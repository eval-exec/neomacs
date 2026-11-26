#!/usr/bin/env python3
"""Local, deterministic ACP fixture; stdin/stdout only, never network/filesystem tools."""
import argparse
import json
import os
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--receipt", required=True)
args = parser.parse_args()
session = "832-local-session"
cwd = os.getcwd()


def receipt(event, **fields):
    with open(args.receipt, "a", encoding="utf-8") as output:
        output.write(json.dumps({"event": event, "pid": os.getpid(), **fields}) + "\n")


def send(message):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", **message}) + "\n")
    sys.stdout.flush()


def state():
    return {
        "sessionId": session,
        "modes": {"currentModeId": "ask", "availableModes": [
            {"id": "ask", "name": "Local fixture", "description": "No tools or network"}]},
        "models": {"currentModelId": "fixture-model", "availableModels": [
            {"modelId": "fixture-model", "name": "Local model"}]},
    }


def history():
    for kind, text in [
        ("user_message_chunk", "Explain this small Java example.\n"),
        ("agent_message_chunk", "# Local resumed session\n\n"
         "This reply comes from the owned ACP fixture.\n\n"
         "```java\nclass Probe { int count = 1; }\n```\n\n"
         "| Item | Value |\n| --- | --- |\n| Local | Ready |\n\n"
         "**Bold**, *italic*, and `inline code`.\n"),
    ]:
        send({"method": "session/update", "params": {
            "sessionId": session,
            "update": {"sessionUpdate": kind, "content": {"type": "text", "text": text}},
        }})


receipt("started")
try:
    for line in sys.stdin:
        request = json.loads(line)
        method = request.get("method")
        receipt("request", method=method, request_id=request.get("id"))
        if "id" not in request:
            if method == "exit":
                break
            continue
        error = None
        if method == "initialize":
            result = {"protocolVersion": 1, "agentInfo": {
                "name": "832 local fixture", "version": "1"}, "authMethods": [],
                "agentCapabilities": {"loadSession": True, "sessionCapabilities": {
                    "list": {}, "resume": {}}}}
        elif method == "authenticate":
            result = {}
        elif method == "session/list":
            result = {"sessions": [{"sessionId": session, "cwd": cwd,
                "title": "832 local resumed session"}]}
        elif method in ("session/load", "session/resume"):
            history()
            result = state()
        elif method == "session/new":
            result = state()
        elif method == "session/prompt":
            history()
            result = {"stopReason": "end_turn"}
        elif method in ("session/set_mode", "session/set_model", "shutdown"):
            result = {}
        else:
            result = None
            error = {"code": -32601, "message": "Fixture has no method: " + str(method)}
        send({"id": request["id"], **({"error": error} if error else {"result": result})})
finally:
    receipt("stdin-closed")
