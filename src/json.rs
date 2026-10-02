// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

//! JSON ↔ [`ZqValue`] bridging (feature `serde_json_support`).
//!
//! Conversions go through the engine's own `JSON` global, so they are exact
//! (numbers, nested arrays/objects, `null`) and reuse the engine's codec rather
//! than duplicating it in the ABI.

use serde_json::Value as JsonValue;

use crate::{ZqCtx, ZqError, ZqValue};

impl<'cx> ZqValue<'cx> {
    /// Build a JS value from a `serde_json::Value` via `JSON.parse`.
    pub fn from_json(cx: ZqCtx<'cx>, val: &JsonValue) -> Result<Self, ZqError> {
        let text = serde_json::to_string(val).unwrap_or_else(|_| "null".to_string());
        cx.json_parse(&text)
    }

    /// Convert this value to a `serde_json::Value` via `JSON.stringify`.
    /// `None` for values with no JSON form (`undefined`, functions, symbols).
    pub fn to_json(&self, cx: ZqCtx<'cx>) -> Option<JsonValue> {
        let s = cx.json_stringify(*self).ok().flatten()?;
        serde_json::from_str(&s).ok()
    }
}
