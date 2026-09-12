#!/usr/bin/env python3
"""Stage a data-only numerical-relativity request in an owned Linux directory.

Called by the trusted desktop adapter, never with generated commands. Execution
and all numerical admission remain the separate supervisor's responsibility.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import uuid

MAX_PACKET = 256 * 1024


def private_root(path: Path) -> Path:
    if not path.is_absolute() or any(part in ("..", ".") for part in path.parts):
        raise ValueError("job root must be a canonical absolute path")
    for part in [*reversed(path.parents), path]:
        info = part.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid not in (0, os.geteuid()):
            raise ValueError("job root has an unowned or non-directory ancestor")
        if info.st_mode & 0o022:
            raise ValueError("job root has a writable ancestor")
    if path.stat().st_uid != os.geteuid() or stat.S_IMODE(path.stat().st_mode) != 0o700:
        raise ValueError("job root must be owned and mode 0700")
    return path


def prepare(job_root: Path, job_id: str, packet: dict) -> dict:
    root = private_root(job_root)
    if str(uuid.UUID(job_id)) != job_id:
        raise ValueError("canonical job UUID required")
    if not isinstance(packet, dict) or set(packet) != {"request", "input_utf8"}:
        raise ValueError("packet must contain exactly request and input_utf8")
    request, text = packet["request"], packet["input_utf8"]
    if not isinstance(request, dict) or not isinstance(text, str):
        raise ValueError("request object and UTF-8 input text required")
    raw = text.encode("utf-8")
    if not raw or len(raw) > 128 * 1024 or b"\0" in raw:
        raise ValueError("input text is empty, oversized or contains NUL")
    directory = root / job_id
    if (request.get("schema") not in ("phaseforge.nr-request.v1", "phaseforge.nr-request.v2")
            or request.get("job_id") != job_id
            or request.get("output_dir") != str(directory / "work")
            or request.get("input") != {"path": "input.athinput", "sha256": hashlib.sha256(raw).hexdigest()}):
        raise ValueError("request identity, output path or input hash differs")
    encoded = (json.dumps(request, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n").encode()
    if len(encoded) > 64 * 1024:
        raise ValueError("request exceeds 64 KiB")
    # Never overwrite a prior attempt, including one interrupted while staging.
    directory.mkdir(mode=0o700, exist_ok=False)
    for name, data in (("input.athinput", raw), ("request.json", encoded)):
        fd = os.open(directory / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
    return {"schema": "phaseforge.nr-staging.v1", "job_id": job_id,
            "directory": str(directory), "request_sha256": hashlib.sha256(encoded).hexdigest(),
            "input_sha256": hashlib.sha256(raw).hexdigest(), "executed": False}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--job-root", required=True, type=Path)
    parser.add_argument("--job-id", required=True)
    args = parser.parse_args()
    try:
        raw = sys.stdin.buffer.read(MAX_PACKET + 1)
        if len(raw) > MAX_PACKET:
            raise ValueError("staging packet exceeds 256 KiB")
        result = prepare(args.job_root, args.job_id, json.loads(raw))
        print(json.dumps(result, allow_nan=False), flush=True)
        return 0
    except (OSError, ValueError, TypeError) as error:
        print(json.dumps({"kind": "rejected", "message": str(error)}), flush=True)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
