// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal BrowserWebGpu reproducer for Cubek extrema reduction identities.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScalarReceipt {
    pub class: String,
    pub bits: u32,
    pub matches_expected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtremaReceipt {
    pub schema: String,
    pub backend: String,
    pub finite_max: ScalarReceipt,
    pub negative_infinity_max: ScalarReceipt,
    pub positive_infinity_min: ScalarReceipt,
    pub nan_max: ScalarReceipt,
    pub all_cases_match: bool,
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use burn::tensor::{Device, DeviceKind, Tensor};
    use wasm_bindgen::prelude::*;

    use super::{ExtremaReceipt, ScalarReceipt};

    fn classify(value: f32, expected: f32) -> ScalarReceipt {
        let matches_expected = if expected.is_nan() {
            value.is_nan()
        } else {
            value.to_bits() == expected.to_bits()
        };
        let class = if value.is_nan() {
            "nan"
        } else if value == f32::NEG_INFINITY {
            "negative_infinity"
        } else if value == f32::INFINITY {
            "positive_infinity"
        } else if value.is_finite() {
            "finite"
        } else {
            "non_finite"
        };
        ScalarReceipt {
            class: class.into(),
            bits: value.to_bits(),
            matches_expected,
        }
    }

    async fn read_scalar(tensor: Tensor<1>) -> Result<f32, JsValue> {
        tensor
            .into_data_async()
            .await
            .map_err(|error| JsValue::from_str(&format!("tensor readback: {error:?}")))?
            .to_vec::<f32>()
            .map_err(|error| JsValue::from_str(&format!("tensor to Vec<f32>: {error:?}")))?
            .into_iter()
            .next()
            .ok_or_else(|| JsValue::from_str("extrema result was empty"))
    }

    async fn max(values: &[f32], device: &Device) -> Result<f32, JsValue> {
        read_scalar(Tensor::<1>::from_floats(values, device).max_dim(0)).await
    }

    async fn min(values: &[f32], device: &Device) -> Result<f32, JsValue> {
        read_scalar(Tensor::<1>::from_floats(values, device).min_dim(0)).await
    }

    #[wasm_bindgen]
    pub async fn run_extrema_repro() -> Result<String, JsValue> {
        let device = Device::wgpu_async(DeviceKind::default()).await;
        let finite_max = classify(max(&[-3.0, 2.0, -1.0], &device).await?, 2.0);
        let negative_infinity_max = classify(
            max(&[f32::NEG_INFINITY, f32::NEG_INFINITY], &device).await?,
            f32::NEG_INFINITY,
        );
        let positive_infinity_min = classify(
            min(&[f32::INFINITY, f32::INFINITY], &device).await?,
            f32::INFINITY,
        );
        let nan_max = classify(max(&[1.0, f32::NAN, 2.0], &device).await?, f32::NAN);
        let all_cases_match = [
            &finite_max,
            &negative_infinity_max,
            &positive_infinity_min,
            &nan_max,
        ]
        .into_iter()
        .all(|case| case.matches_expected);
        let receipt = ExtremaReceipt {
            schema: "distillery.cubek-browser-extrema-repro/v1".into(),
            backend: "burn-wgpu/browser-webgpu".into(),
            finite_max,
            negative_infinity_max,
            positive_infinity_min,
            nan_max,
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
        let scalar = ScalarReceipt {
            class: "finite".into(),
            bits: 2.0_f32.to_bits(),
            matches_expected: true,
        };
        let receipt = ExtremaReceipt {
            schema: "distillery.cubek-browser-extrema-repro/v1".into(),
            backend: "test".into(),
            finite_max: scalar.clone(),
            negative_infinity_max: scalar.clone(),
            positive_infinity_min: scalar.clone(),
            nan_max: scalar,
            all_cases_match: true,
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert_eq!(
            serde_json::from_str::<ExtremaReceipt>(&json).unwrap(),
            receipt
        );
    }
}
