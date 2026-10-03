#!/usr/bin/env python3
"""Run only against a disposable X11 session; replaces its clipboard."""
import base64
import json
import selectors
import subprocess
import sys

worker = sys.argv[1]
children = []


def item(kind, data):
    return {"type": kind, "data": base64.b64encode(data).decode("ascii")}


def request(operation, representations=()):
    return json.dumps({"operation": operation, "representations": representations}).encode()


def read(picture=False):
    completed = subprocess.run(
        [worker], input=request("read_picture" if picture else "read"),
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10, check=True,
    )
    return json.loads(completed.stdout)["representations"]


def publish(representations):
    process = subprocess.Popen(
        [worker], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    children.append(process)
    process.stdin.write(request("write", representations))
    process.stdin.close()
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        assert selector.select(10), "No publication acknowledgement"
    assert json.loads(process.stdout.readline()) == {"representations": []}
    process.stdout.close()
    return process


try:
    native = item("dev.reshiki.drawing", b'{"snapshot":"first"}')
    png = item("public.png", bytes(range(256)) * 8192)
    first = publish([native, png])
    assert first.poll() is None, "Owner exited after acknowledging the copy"
    assert read() == [native]
    assert read(picture=True) == [png]

    for invalid in [[], [native, {"type": "public.png", "data": "?"}],
                    [native, item("unsupported/type", b"bad")]]:
        failed = subprocess.run(
            [worker], input=request("write", invalid), stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, timeout=10,
        )
        assert failed.returncode != 0
        assert "error" in json.loads(failed.stdout)
        assert read() == [native], "Rejected write replaced the existing selection"

    # Cancellation before the complete request cannot acquire selection ownership.
    pending = subprocess.Popen([worker], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    children.append(pending)
    pending.stdin.write(b'{"operation":"write",')
    pending.stdin.flush()
    pending.kill()
    pending.wait(timeout=2)
    pending.stdin.close()
    pending.stdout.close()
    assert read() == [native]

    replacement = item("public.utf8-plain-text", "CCO 日本語".encode())
    publish([replacement])
    first.wait(timeout=7)
    assert first.returncode == 0
    assert read() == [replacement]
    assert read(picture=True) == []
    print("worker smoke: persistence, aliases, INCR, atomic rejection, cancellation and replacement passed")
finally:
    for process in children:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=2)
        if process.stderr is not None:
            process.stderr.close()
