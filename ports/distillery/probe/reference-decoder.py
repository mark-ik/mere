# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Emit an independent Transformers reference for the configured decoder row."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import torch
import transformers
from transformers import AutoModelForCausalLM, AutoTokenizer


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("model_dir", type=Path)
    parser.add_argument("prompt")
    parser.add_argument("--max-tokens", type=int, default=8)
    arguments = parser.parse_args()

    tokenizer = AutoTokenizer.from_pretrained(arguments.model_dir, local_files_only=True)
    model = AutoModelForCausalLM.from_pretrained(
        arguments.model_dir,
        local_files_only=True,
        dtype=torch.float32,
    )
    model.eval()
    encoded = tokenizer(arguments.prompt, add_special_tokens=True, return_tensors="pt")
    with torch.no_grad():
        output = model.generate(
            **encoded,
            do_sample=False,
            max_new_tokens=arguments.max_tokens,
            use_cache=True,
        )[0]
    prompt_tokens = encoded["input_ids"].shape[1]
    generated = output[prompt_tokens:].to(torch.int64).tolist()

    print(
        json.dumps(
            {
                "schema": "distillery.transformers-decoder-reference/v1",
                "model_dir": str(arguments.model_dir),
                "prompt": arguments.prompt,
                "max_tokens": arguments.max_tokens,
                "prompt_token_ids": encoded["input_ids"][0].to(torch.int64).tolist(),
                "generated_token_ids": generated,
                "generated_text": tokenizer.decode(generated, skip_special_tokens=True),
                "reference_engine": {
                    "torch": torch.__version__,
                    "transformers": transformers.__version__,
                    "dtype": "float32",
                    "device": "cpu",
                },
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
