#!/usr/bin/python3
"""Local, read-only merchant service review example; no network is used."""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any

from anyflows_plugin import Plugin, PluginError


ROOT = Path(__file__).resolve().parent
CAPABILITY = "merchant.service.review.read"
MODEL_FIELDS = {
    "model",
    "context_window",
    "max_output_tokens",
    "supports_streaming",
    "supports_tools",
    "supports_vision",
}


def review_service(capability_id: str, public_config: dict[str, Any], value: object) -> dict[str, Any]:
    if capability_id != CAPABILITY:
        raise PluginError("unsupported")
    if public_config or not isinstance(value, dict) or set(value) != {"service"}:
        raise PluginError("invalid_configuration")

    service = value["service"]
    if not isinstance(service, dict) or set(service) != {"name", "description", "protocol", "models"}:
        raise PluginError("invalid_configuration")
    if not all(isinstance(service[key], str) for key in ("name", "description", "protocol")):
        raise PluginError("invalid_configuration")
    models = service["models"]
    if not isinstance(models, list) or len(models) > 128:
        raise PluginError("invalid_configuration")
    for model in models:
        if not isinstance(model, dict) or set(model) != MODEL_FIELDS:
            raise PluginError("invalid_configuration")

    checks: list[dict[str, Any]] = []
    if not models:
        checks.append({
            "code": "models.empty",
            "title": "No models configured",
            "status": "warning",
            "message": "This service does not list any models yet.",
            "recommendation": "Add at least one model before submitting the service.",
        })
    else:
        checks.append({
            "code": "models.present",
            "title": "Models are listed",
            "status": "pass",
            "message": f"The service lists {len(models)} model(s).",
            "recommendation": None,
        })

    named_models = [model for model in models if isinstance(model.get("model"), str) and model["model"].strip()]
    if len(named_models) != len(models):
        checks.append({
            "code": "models.names",
            "title": "Model names need attention",
            "status": "warning",
            "message": "At least one listed model has a blank name.",
            "recommendation": "Give each model a non-empty public model ID.",
        })
    else:
        checks.append({
            "code": "models.names",
            "title": "Model names are present",
            "status": "pass",
            "message": "Every listed model has a public model ID.",
            "recommendation": None,
        })

    missing_context = [model["model"] for model in models if model.get("context_window") is None]
    if missing_context:
        checks.append({
            "code": "models.context-window",
            "title": "Context windows are incomplete",
            "status": "info",
            "message": f"{len(missing_context)} model(s) do not specify a context window.",
            "recommendation": "Add context-window details when they are known.",
        })
    else:
        checks.append({
            "code": "models.context-window",
            "title": "Context windows are specified",
            "status": "pass",
            "message": "Every listed model specifies a context window.",
            "recommendation": None,
        })

    description = service["description"].strip()
    if len(description) < 20:
        checks.append({
            "code": "service.description",
            "title": "Service description is brief",
            "status": "info",
            "message": "The description may not explain the service clearly enough to buyers.",
            "recommendation": "Describe the service behavior and notable model capabilities.",
        })
    else:
        checks.append({
            "code": "service.description",
            "title": "Service description is present",
            "status": "pass",
            "message": "The description contains useful detail.",
            "recommendation": None,
        })

    summary = f"Reviewed {service['name']} ({service['protocol']}) and found {len(checks)} documentation checks."
    result = {"summary": summary, "checks": checks}
    # Keep this example comfortably within the host response ceiling.
    if len(json.dumps(result, ensure_ascii=False).encode("utf-8")) > 16 * 1024:
        raise PluginError("invalid_response")
    return result


def main() -> int:
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    return Plugin(manifest, review_service).serve()


if __name__ == "__main__":
    sys.exit(main())
