# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Emit independent PyTorch/Transformers references for the D2c model matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path

import torch
import torch.nn.functional as functional
import transformers
from transformers import AutoModel, AutoTokenizer


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--matrix", type=Path, default=Path(__file__).with_name("model-matrix.json"))
    parser.add_argument("--model")
    arguments = parser.parse_args()

    matrix = json.loads(arguments.matrix.read_text(encoding="utf-8"))
    mere_root = arguments.matrix.resolve().parents[3]
    results = []
    for row in matrix["models"]:
        if arguments.model and row["model_id"] != arguments.model:
            continue
        model_dir = mere_root / row["model_base_url"].removeprefix("/")
        tokenizer = AutoTokenizer.from_pretrained(model_dir, local_files_only=True)
        model = AutoModel.from_pretrained(model_dir, local_files_only=True, add_pooling_layer=False)
        model.eval()
        encoded = tokenizer(
            [matrix["input"]],
            max_length=model.config.max_position_embeddings,
            padding=True,
            truncation=True,
            return_tensors="pt",
        )
        with torch.no_grad():
            hidden = model(**encoded).last_hidden_state
            mask = encoded["attention_mask"].unsqueeze(-1).expand(hidden.size()).float()
            pooled = (hidden * mask).sum(dim=1) / mask.sum(dim=1).clamp(min=1e-9)
            vector = functional.normalize(pooled, p=2, dim=1)[0].to(torch.float32).tolist()
        output_bytes = b"".join(struct.pack("<f", value) for value in vector)
        results.append(
            {
                "model_id": row["model_id"],
                "revision": row["revision"],
                "pooling": "attention-mask-aware mean",
                "dimensions": len(vector),
                "l2_norm": sum(value * value for value in vector) ** 0.5,
                "first_8": vector[:8],
                "output_f32le_sha256": hashlib.sha256(output_bytes).hexdigest(),
                "reference_engine": {
                    "torch": torch.__version__,
                    "transformers": transformers.__version__,
                    "device": "cpu",
                },
            }
        )
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
