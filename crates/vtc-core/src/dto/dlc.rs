//! DLC DTO（对齐 VTC.Shared.DlcDtos）

use serde::{Deserialize, Serialize};

/// 地图类文件名清单（响应 = 行数组）
pub type DlcMapTemplate = Vec<String>;

/// 车队要求 DLC 列表项
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DlcRequiredDto {
    pub dlc_id: String,
    pub display_name: String,
    pub is_map: bool,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase_url: Option<String>,
}

/// DLC 状态上报请求
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DlcReportDto {
    #[serde(default = "default_game")]
    pub game: String,
    #[serde(default)]
    pub missing: Vec<String>,
    #[serde(default)]
    pub installed: Vec<String>,
}
fn default_game() -> String { "ets2".into() }

/// DLC 禁用请求
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DlcDisableReq {
    #[serde(default = "default_game")]
    pub game: String,
    #[serde(default = "default_major")]
    pub version: String,
    /// 逗号分隔的豁免包（活动 requires_map_dlc 标签）
    pub requires_map_dlc: Option<String>,
}
fn default_major() -> String { "1.61".into() }

/// 禁用计划（响应，客户端据此执行 move）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DlcDisablePlan {
    pub backup_dir: String,
    /// 需要 move 走的文件
    pub to_disable: Vec<String>,
    /// 豁免保留的文件
    pub keep: Vec<String>,
}

/// 禁用计划请求参数（内部）
#[derive(Debug, Clone)]
pub struct PlanDisableArgs {
    pub user_id: String,
    pub game: String,
    pub major: String,
    /// 豁免包（逗号分隔 → 切分）
    pub requires_map_dlc: Option<String>,
    pub ip: String,
}
