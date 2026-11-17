#!/usr/bin/env python3
"""Run a local plugin through the contract v1 handshake and one capability call."""

from __future__ import annotations

import argparse
import asyncio
import json
import os
from pathlib import Path
import sys
import time
from typing import Any


MAX_FRAME_BYTES = 1_048_576
SDK_DIRECTORY = Path(__file__).resolve().parent / "sdk" / "python"


def read_json(path: Path | None, default: Any = None) -> Any:
    if path is None:
        return default
    with path.open(encoding="utf-8") as source:
        return json.load(source)


def frame(value: dict[str, Any]) -> bytes:
    return (json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")


def envelope(manifest: dict[str, Any], operation: dict[str, Any], request_id: str) -> dict[str, Any]:
    return {
        "protocolVersion": 1,
        "pluginId": manifest["pluginId"],
        "pluginVersion": manifest["pluginVersion"],
        "artifactSha256": "a" * 64,
        "deploymentGeneration": 1,
        "requestId": request_id,
        "deadlineUnixMs": int(time.time() * 1000) + 30_000,
        "operation": operation,
    }


def continuation(proxy_request: dict[str, Any], payload: Any) -> dict[str, Any]:
    fields = {
        key: proxy_request[key]
        for key in (
            "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
            "deploymentGeneration", "requestId", "capabilityId", "capabilityVersion",
            "proxyRequestId",
        )
    }
    return fields | {
        "result": {"kind": "response", "value": {"statusCode": 200, "payload": payload}}
    }


async def read_frame(process: asyncio.subprocess.Process) -> dict[str, Any]:
    assert process.stdout is not None
    raw = await asyncio.wait_for(process.stdout.readline(), timeout=10)
    if not raw:
        raise RuntimeError("插件提前关闭了标准输出")
    if len(raw) > MAX_FRAME_BYTES or not raw.endswith(b"\n"):
        raise RuntimeError("插件返回的协议帧超过 1 MiB 或缺少换行")
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeError("插件返回的不是有效 JSON") from error
    if not isinstance(value, dict):
        raise RuntimeError("插件返回的协议帧必须是 JSON 对象")
    return value


async def exchange(
    process: asyncio.subprocess.Process,
    request: dict[str, Any],
    proxy_payload: Any,
) -> dict[str, Any]:
    assert process.stdin is not None
    process.stdin.write(frame(request))
    await process.stdin.drain()
    while True:
        response = await read_frame(process)
        if "proxyRequestId" not in response:
            for key in (
                "protocolVersion", "pluginId", "pluginVersion", "artifactSha256",
                "deploymentGeneration", "requestId",
            ):
                if response.get(key) != request.get(key):
                    raise RuntimeError(f"响应关联字段不匹配：{key}")
            return response
        if proxy_payload is None:
            raise RuntimeError(
                "插件请求了宿主回调；请使用 --proxy-response 提供 JSON 回调结果"
            )
        process.stdin.write(frame(continuation(response, proxy_payload)))
        await process.stdin.drain()


async def run(args: argparse.Namespace) -> int:
    root = args.root.resolve()
    manifest = read_json(args.manifest.resolve())
    if not isinstance(manifest, dict):
        raise RuntimeError("Manifest 必须是 JSON 对象")
    capabilities = manifest.get("capabilities")
    if not isinstance(capabilities, list):
        raise RuntimeError("Manifest 缺少 capabilities")
    declaration = next(
        (item for item in capabilities if item.get("id") == args.capability), None
    )
    if not isinstance(declaration, dict):
        raise RuntimeError(f"Manifest 中没有 capability：{args.capability}")

    entry = (root / args.entry).resolve()
    if not entry.is_file():
        raise RuntimeError(f"找不到插件入口：{entry}")
    command = [sys.executable, str(entry)] if entry.suffix.lower() == ".py" else [str(entry)]
    environment = os.environ.copy()
    environment["PYTHONPATH"] = os.pathsep.join(
        item for item in (str(SDK_DIRECTORY), environment.get("PYTHONPATH", "")) if item
    )
    process = await asyncio.create_subprocess_exec(
        *command,
        cwd=str(entry.parent),
        env=environment,
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    input_value = read_json(args.input, {})
    config = read_json(args.config, {})
    proxy_payload = read_json(args.proxy_response)
    try:
        manifest_response = await exchange(
            process, envelope(manifest, {"operation": "manifest"}, f"{args.request_id}-manifest"), proxy_payload
        )
        if manifest_response.get("result", {}).get("kind") != "manifest":
            raise RuntimeError(f"manifest 握手失败：{json.dumps(manifest_response, ensure_ascii=False)}")

        health_response = await exchange(
            process, envelope(manifest, {"operation": "health"}, f"{args.request_id}-health"), proxy_payload
        )
        if health_response.get("result", {}).get("kind") != "health":
            raise RuntimeError(f"health 握手失败：{json.dumps(health_response, ensure_ascii=False)}")

        capability_response = await exchange(
            process,
            envelope(
                manifest,
                {
                    "operation": "invoke_capability",
                    "connectionId": args.connection_id,
                    "publicConfig": config,
                    "capabilityId": args.capability,
                    "capabilityVersion": declaration["version"],
                    "input": input_value,
                },
                args.request_id,
            ),
            proxy_payload,
        )
        result = capability_response.get("result")
        if not isinstance(result, dict) or result.get("kind") not in {"capability", "error"}:
            raise RuntimeError(f"能力调用返回格式错误：{json.dumps(capability_response, ensure_ascii=False)}")
        if result["kind"] == "error" and not args.allow_error:
            raise RuntimeError(f"插件返回业务错误：{json.dumps(result, ensure_ascii=False)}")
        if args.expect:
            expected = read_json(args.expect)
            actual = result.get("value", {}).get("output")
            if actual != expected:
                raise RuntimeError(
                    "能力输出与期望文件不一致：\n"
                    + json.dumps({"expected": expected, "actual": actual}, ensure_ascii=False, indent=2)
                )
        print(json.dumps({"manifest": "ok", "health": "ok", "result": result["kind"]}, ensure_ascii=False))
        return 0
    finally:
        if process.stdin:
            process.stdin.close()
        try:
            await asyncio.wait_for(process.wait(), timeout=3)
        except asyncio.TimeoutError:
            process.kill()
            await process.wait()
        if process.returncode != 0:
            assert process.stderr is not None
            error = (await process.stderr.read()).decode("utf-8", errors="replace").strip()
            detail = f"；stderr：{error}" if error else ""
            raise RuntimeError(f"插件进程退出码为 {process.returncode}{detail}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True, help="插件目录")
    parser.add_argument("--manifest", type=Path, required=True, help="Manifest JSON 文件")
    parser.add_argument("--entry", default="plugin.py", help="相对插件目录的入口文件")
    parser.add_argument("--capability", required=True, help="要测试的 capability ID")
    parser.add_argument("--input", type=Path, required=True, help="能力输入 JSON 文件")
    parser.add_argument("--config", type=Path, help="publicConfig JSON 文件，默认 {}")
    parser.add_argument("--expect", type=Path, help="期望的能力输出 JSON 文件")
    parser.add_argument("--proxy-response", type=Path, help="宿主回调返回的 JSON 投影")
    parser.add_argument("--allow-error", action="store_true", help="允许能力调用返回 error，用于测试拒绝分支")
    parser.add_argument("--connection-id", default="local-test")
    parser.add_argument("--request-id", default="local-plugin-test")
    args = parser.parse_args()
    try:
        return asyncio.run(run(args))
    except (OSError, RuntimeError, json.JSONDecodeError, KeyError, TypeError) as error:
        parser.exit(1, f"test-plugin: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
