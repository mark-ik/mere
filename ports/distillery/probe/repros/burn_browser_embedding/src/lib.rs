// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal BrowserWebGpu reproducer for Burn's embedding/select path.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingCaseReceipt {
    pub name: String,
    pub table_shape: [usize; 2],
    pub indices: Vec<i32>,
    pub prior_uploads: usize,
    pub prior_upload_elements: usize,
    pub queued_downstream_ops: usize,
    pub through_param_module: bool,
    pub input_round_trip_matches: bool,
    pub output_len: usize,
    pub expected_len: usize,
    pub all_finite: bool,
    pub nan_count: usize,
    pub first_non_finite_index: Option<usize>,
    pub first_8: Vec<f32>,
    pub first_8_bits: Vec<u32>,
    pub first_mismatch_index: Option<usize>,
    pub matches_expected: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingReceipt {
    pub schema: String,
    pub backend: String,
    pub cases: Vec<EmbeddingCaseReceipt>,
    pub graph_cases: Vec<GraphCaseReceipt>,
    pub all_cases_match: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphCaseReceipt {
    pub name: String,
    pub shape: [usize; 3],
    pub output_len: usize,
    pub all_finite: bool,
    pub nan_count: usize,
    pub first_non_finite_index: Option<usize>,
    pub first_8: Vec<f32>,
    pub first_8_bits: Vec<u32>,
    pub maximum_row_mean_abs: f32,
    pub maximum_row_variance_error: f32,
    pub matches_expected: bool,
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use burn::nn::LayerNormConfig;
    use burn::tensor::{Device, DeviceKind, Int, Tensor, TensorData, module::embedding};
    use wasm_bindgen::prelude::*;

    use super::{EmbeddingCaseReceipt, EmbeddingReceipt, GraphCaseReceipt};

    struct PendingEmbeddingCase {
        name: &'static str,
        table_shape: [usize; 2],
        indices: Vec<i32>,
        expected: Vec<f32>,
        input_probe: Tensor<2, Int>,
        output: Tensor<3>,
    }

    fn queue_case(
        name: &'static str,
        rows: usize,
        columns: usize,
        indices: &[i32],
        device: &Device,
    ) -> PendingEmbeddingCase {
        let weight_values = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (row * 1_000 + column) as f32))
            .collect::<Vec<_>>();
        let expected = indices
            .iter()
            .flat_map(|index| {
                let start = *index as usize * columns;
                weight_values[start..start + columns].iter().copied()
            })
            .collect::<Vec<_>>();
        let weights: Tensor<2> =
            Tensor::from_data(TensorData::new(weight_values, [rows, columns]), device);
        let input: Tensor<2, Int> = Tensor::from_data(
            TensorData::new(indices.to_vec(), [1, indices.len()]),
            device,
        );
        let output = embedding(weights, input.clone());
        PendingEmbeddingCase {
            name,
            table_shape: [rows, columns],
            indices: indices.to_vec(),
            expected,
            input_probe: input,
            output,
        }
    }

    async fn finish_queued_case(
        case: PendingEmbeddingCase,
    ) -> Result<EmbeddingCaseReceipt, JsValue> {
        let input_readback = case
            .input_probe
            .into_data_async()
            .await
            .map_err(|error| JsValue::from_str(&format!("group input readback: {error:?}")))?
            .to_vec::<i32>()
            .map_err(|error| JsValue::from_str(&format!("group input to Vec<i32>: {error:?}")))?;
        let output = case
            .output
            .into_data_async()
            .await
            .map_err(|error| JsValue::from_str(&format!("group embedding readback: {error:?}")))?
            .to_vec::<f32>()
            .map_err(|error| {
                JsValue::from_str(&format!("group embedding to Vec<f32>: {error:?}"))
            })?;
        let first_mismatch_index = output
            .iter()
            .zip(&case.expected)
            .position(|(actual, expected)| actual.to_bits() != expected.to_bits());
        let first_8 = output.iter().take(8).copied().collect::<Vec<_>>();
        let first_8_bits = first_8.iter().map(|value| value.to_bits()).collect();
        let all_finite = output.iter().all(|value| value.is_finite());
        let nan_count = output.iter().filter(|value| value.is_nan()).count();
        let first_non_finite_index = output.iter().position(|value| !value.is_finite());
        let matches_expected =
            output.len() == case.expected.len() && first_mismatch_index.is_none();
        Ok(EmbeddingCaseReceipt {
            name: case.name.into(),
            table_shape: case.table_shape,
            indices: case.indices.clone(),
            prior_uploads: 0,
            prior_upload_elements: 0,
            queued_downstream_ops: 1,
            through_param_module: false,
            input_round_trip_matches: input_readback == case.indices,
            output_len: output.len(),
            expected_len: case.expected.len(),
            all_finite,
            nan_count,
            first_non_finite_index,
            first_8,
            first_8_bits,
            first_mismatch_index,
            matches_expected,
        })
    }

    async fn run_grouped_lookups(device: &Device) -> Result<Vec<EmbeddingCaseReceipt>, JsValue> {
        let word = queue_case(
            "grouped-minilm-word",
            30_522,
            384,
            &[101, 2023, 2003, 1037, 7099, 6251, 1012, 102],
            device,
        );
        let position = queue_case(
            "grouped-minilm-position",
            512,
            384,
            &[0, 1, 2, 3, 4, 5, 6, 7],
            device,
        );
        let token_type = queue_case(
            "grouped-minilm-token-type",
            2,
            384,
            &[0, 0, 0, 0, 0, 0, 0, 0],
            device,
        );
        let combined = word.output.clone() + position.output.clone() + token_type.output.clone();
        let cases = vec![
            finish_queued_case(word).await?,
            finish_queued_case(position).await?,
            finish_queued_case(token_type).await?,
        ];
        let _combined = combined
            .into_data_async()
            .await
            .map_err(|error| JsValue::from_str(&format!("group sum readback: {error:?}")))?;
        Ok(cases)
    }

    async fn run_case(
        name: &str,
        rows: usize,
        columns: usize,
        indices: &[i32],
        prior_uploads: usize,
        prior_upload_elements: usize,
        queued_downstream_ops: usize,
        through_param_module: bool,
        device: &Device,
    ) -> Result<EmbeddingCaseReceipt, JsValue> {
        let weight_values = (0..rows)
            .flat_map(|row| (0..columns).map(move |column| (row * 1_000 + column) as f32))
            .collect::<Vec<_>>();
        let weights: Tensor<2> = Tensor::from_data(
            TensorData::new(weight_values.clone(), [rows, columns]),
            device,
        );
        let held_uploads = (0..prior_uploads)
            .map(|upload| {
                Tensor::<1>::from_data(
                    TensorData::new(
                        vec![upload as f32; prior_upload_elements],
                        [prior_upload_elements],
                    ),
                    device,
                )
            })
            .collect::<Vec<_>>();
        let input: Tensor<2, Int> = Tensor::from_data(
            TensorData::new(indices.to_vec(), [1, indices.len()]),
            device,
        );
        let input_readback = input
            .clone()
            .into_data_async()
            .await
            .map_err(|error| JsValue::from_str(&format!("input readback: {error:?}")))?
            .to_vec::<i32>()
            .map_err(|error| JsValue::from_str(&format!("input to Vec<i32>: {error:?}")))?;
        let output_tensor = if through_param_module {
            let module = burn::nn::Embedding {
                weight: burn::module::Param::from_tensor(weights),
            };
            module.forward(input)
        } else {
            embedding(weights, input)
        };
        let mut downstream = output_tensor.clone();
        for _ in 0..queued_downstream_ops {
            downstream = downstream + 1.0;
        }
        let output = output_tensor
            .into_data_async()
            .await
            .map_err(|error| JsValue::from_str(&format!("embedding readback: {error:?}")))?
            .to_vec::<f32>()
            .map_err(|error| JsValue::from_str(&format!("embedding to Vec<f32>: {error:?}")))?;
        let _downstream = downstream
            .into_data_async()
            .await
            .map_err(|error| JsValue::from_str(&format!("downstream readback: {error:?}")))?;
        let expected = indices
            .iter()
            .flat_map(|index| {
                let start = *index as usize * columns;
                weight_values[start..start + columns].iter().copied()
            })
            .collect::<Vec<_>>();
        let first_mismatch_index = output
            .iter()
            .zip(&expected)
            .position(|(actual, expected)| actual.to_bits() != expected.to_bits());
        let first_8 = output.iter().take(8).copied().collect::<Vec<_>>();
        let first_8_bits = first_8.iter().map(|value| value.to_bits()).collect();
        let all_finite = output.iter().all(|value| value.is_finite());
        let nan_count = output.iter().filter(|value| value.is_nan()).count();
        let first_non_finite_index = output.iter().position(|value| !value.is_finite());
        let matches_expected = output.len() == expected.len() && first_mismatch_index.is_none();
        drop(held_uploads);

        Ok(EmbeddingCaseReceipt {
            name: name.into(),
            table_shape: [rows, columns],
            indices: indices.to_vec(),
            prior_uploads,
            prior_upload_elements,
            queued_downstream_ops,
            through_param_module,
            input_round_trip_matches: input_readback == indices,
            output_len: output.len(),
            expected_len: expected.len(),
            all_finite,
            nan_count,
            first_non_finite_index,
            first_8,
            first_8_bits,
            first_mismatch_index,
            matches_expected,
        })
    }

    async fn run_layer_norm_case(device: &Device) -> Result<GraphCaseReceipt, JsValue> {
        const ROWS: usize = 8;
        const WIDTH: usize = 384;
        let values = (0..ROWS * WIDTH)
            .map(|index| ((index % 31) as f32 - 15.0) / 7.0)
            .collect::<Vec<_>>();
        let input: Tensor<3> = Tensor::from_data(TensorData::new(values, [1, ROWS, WIDTH]), device);
        let output = LayerNormConfig::new(WIDTH)
            .init(device)
            .forward(input)
            .into_data_async()
            .await
            .map_err(|error| JsValue::from_str(&format!("layer norm readback: {error:?}")))?
            .to_vec::<f32>()
            .map_err(|error| JsValue::from_str(&format!("layer norm to Vec<f32>: {error:?}")))?;
        let first_8 = output.iter().take(8).copied().collect::<Vec<_>>();
        let first_8_bits = first_8.iter().map(|value| value.to_bits()).collect();
        let all_finite = output.iter().all(|value| value.is_finite());
        let nan_count = output.iter().filter(|value| value.is_nan()).count();
        let first_non_finite_index = output.iter().position(|value| !value.is_finite());
        let mut maximum_row_mean_abs = 0.0_f32;
        let mut maximum_row_variance_error = 0.0_f32;
        for row in output.chunks_exact(WIDTH) {
            let mean = row.iter().sum::<f32>() / WIDTH as f32;
            let variance = row
                .iter()
                .map(|value| {
                    let centered = *value - mean;
                    centered * centered
                })
                .sum::<f32>()
                / WIDTH as f32;
            maximum_row_mean_abs = maximum_row_mean_abs.max(mean.abs());
            maximum_row_variance_error = maximum_row_variance_error.max((variance - 1.0).abs());
        }
        let matches_expected = output.len() == ROWS * WIDTH
            && all_finite
            && maximum_row_mean_abs < 1.0e-4
            && maximum_row_variance_error < 1.0e-3;
        Ok(GraphCaseReceipt {
            name: "bert-width-layer-norm".into(),
            shape: [1, ROWS, WIDTH],
            output_len: output.len(),
            all_finite,
            nan_count,
            first_non_finite_index,
            first_8,
            first_8_bits,
            maximum_row_mean_abs,
            maximum_row_variance_error,
            matches_expected,
        })
    }

    #[wasm_bindgen]
    pub async fn run_embedding_repro() -> Result<String, JsValue> {
        let device = Device::wgpu_async(DeviceKind::default()).await;
        let mut cases = vec![
            run_case("tiny-mixed", 4, 3, &[2, 0, 3, 1], 0, 0, 0, false, &device).await?,
            run_case(
                "bert-width-mixed",
                16,
                384,
                &[1, 7, 3, 12, 0, 15, 2, 9],
                0,
                0,
                0,
                false,
                &device,
            )
            .await?,
            run_case(
                "bert-width-zeros",
                2,
                384,
                &[0, 0, 0, 0, 0, 0, 0, 0],
                0,
                0,
                0,
                false,
                &device,
            )
            .await?,
            run_case(
                "bert-width-after-bulk-uploads",
                16,
                384,
                &[1, 7, 3, 12, 0, 15, 2, 9],
                100,
                147_456,
                0,
                false,
                &device,
            )
            .await?,
            run_case(
                "minilm-word-table-after-model-sized-uploads",
                30_522,
                384,
                &[101, 2023, 2003, 1037, 7099, 6251, 1012, 102],
                100,
                110_000,
                0,
                false,
                &device,
            )
            .await?,
            run_case(
                "minilm-word-table-with-queued-consumers",
                30_522,
                384,
                &[101, 2023, 2003, 1037, 7099, 6251, 1012, 102],
                100,
                110_000,
                128,
                false,
                &device,
            )
            .await?,
            run_case(
                "minilm-word-table-through-param-module",
                30_522,
                384,
                &[101, 2023, 2003, 1037, 7099, 6251, 1012, 102],
                100,
                110_000,
                0,
                true,
                &device,
            )
            .await?,
            run_case(
                "minilm-word-table-after-small-fragment-uploads",
                30_522,
                384,
                &[101, 2023, 2003, 1037, 7099, 6251, 1012, 102],
                100,
                384,
                0,
                true,
                &device,
            )
            .await?,
        ];
        cases.extend(run_grouped_lookups(&device).await?);
        let graph_cases = vec![run_layer_norm_case(&device).await?];
        let all_cases_match = cases.iter().all(|case| case.matches_expected)
            && graph_cases.iter().all(|case| case.matches_expected);
        let receipt = EmbeddingReceipt {
            schema: "distillery.burn-browser-embedding-repro/v2".into(),
            backend: "burn-wgpu/browser-webgpu".into(),
            cases,
            graph_cases,
            all_cases_match,
        };
        serde_json::to_string_pretty(&receipt)
            .map_err(|error| JsValue::from_str(&format!("serialize receipt: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_contract_round_trips() {
        let receipt = EmbeddingReceipt {
            schema: "distillery.burn-browser-embedding-repro/v2".into(),
            backend: "test".into(),
            cases: Vec::new(),
            graph_cases: Vec::new(),
            all_cases_match: true,
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert_eq!(
            serde_json::from_str::<EmbeddingReceipt>(&json).unwrap(),
            receipt
        );
    }
}
