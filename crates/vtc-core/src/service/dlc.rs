//! DLC 服务（S6）：模板加载 / 状态上报 / 禁用计划 / 清单
//! 地图类整名匹配由**客户端**执行；平台只发计划 + 审计 + 备份清单（§3.8）

use crate::backend::Db;
use crate::constants::{audit, game};
use crate::domain::{DlcEntry, DlcReport, DlcState};
use crate::dto::dlc::{DlcDisablePlan, DlcDisableReq, DlcMapTemplate, DlcRequiredDto};
use crate::error::{AppError, AppResult};
use std::path::Path;
use std::sync::Arc;

/// DLC 服务：泛型 over `Db`（只依赖 db），持有具体 `Arc<D>`。
pub struct DlcService<D: Db> {
    pub db: Arc<D>,
    /// 地图 DLC 模板目录（map-dlc/），含 <major>.<game>.txt
    pub template_dir: String,
}

/// 从**任意来源的模板文本**解析出行数组（纯函数，无文件系统/后端依赖 →
/// worker 可从 R2/DB 读出文本后直接解析，local 读本地文件后解析）。
/// 行 = 每行 trim、去空行、去 `#` 注释行。
pub fn parse_template(content: &str) -> DlcMapTemplate {
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

impl<D: Db> DlcService<D> {
    pub fn new(db: Arc<D>, template_dir: String) -> Self {
        DlcService { db, template_dir }
    }

    /// 加载某大版本+游戏的地图类文件名清单（行数组）。
    /// **读本地文件**（Docker/local 形态）；worker 形态无文件系统，改走 `map_template_from`（由
    /// 路由层从 R2/DB 读出模板文本后传入）。
    pub fn load_template(&self, major: &str, gamename: &str) -> AppResult<DlcMapTemplate> {
        let path = Path::new(&self.template_dir).join(format!("{major}.{gamename}.txt"));
        let content = std::fs::read_to_string(&path)
            .map_err(|_| AppError::NotFound(format!("地图 DLC 模板缺失：{}（请放 {}）", path.display(), self.template_dir)))?;
        Ok(parse_template(&content))
    }

    /// 地图类文件名清单（GET /dlc/map-template?game&major）。
    /// `source` = 模板文本（local 形态由路由 `load_template` 读文件后传入；worker 由 R2/DB 读出）。
    /// 这样核心逻辑不碰文件系统，双形态都能用。
    pub fn map_template_from(&self, source: &str) -> AppResult<DlcMapTemplate> {
        Ok(parse_template(source))
    }

    /// 地图类文件名清单（GET /dlc/map-template?game&major）· local 便利重载。
    pub fn map_template(&self, gamename: &str, major: &str) -> AppResult<DlcMapTemplate> {
        self.load_template(major, gamename)
    }

    /// 车队要求 DLC 列表（GET /dlc/required）
    pub async fn required(&self, gamename: &str, major: &str) -> AppResult<Vec<DlcRequiredDto>> {
        let entries = self.db.list_dlc_required(gamename, major).await?;
        Ok(entries
            .into_iter()
            .map(|e| DlcRequiredDto {
                dlc_id: e.dlc_id,
                display_name: e.display_name,
                is_map: e.is_map,
                required: e.required,
                purchase_url: e.purchase_url,
            })
            .collect())
    }

    /// 状态上报（客户端）
    pub async fn report(&self, member_id: &str, game: &str, missing: &[String], installed: &[String]) -> AppResult<()> {
        let rep = DlcReport {
            id: uuid::Uuid::new_v4().to_string(),
            member_id: member_id.into(),
            game: game.into(),
            missing: serde_json::to_string(missing).unwrap_or_default(),
            installed: serde_json::to_string(installed).unwrap_or_default(),
            reported_at: chrono::Utc::now(),
        };
        self.db.insert_dlc_report(&rep).await
    }

    /// 生成禁用计划（POST /dlc/disable）· local 重载（读本地模板文件）。
    pub async fn plan_disable(&self, actor: &str, ip: &str, req: &DlcDisableReq) -> AppResult<DlcDisablePlan> {
        let template = self.load_template(&req.version, &req.game)?;
        self.plan_disable_from(template, actor, ip, req).await
    }

    /// 生成禁用计划（POST /dlc/disable）· 从**模板行数组**出发（无文件系统依赖）。
    /// local 形态 `plan_disable` 读文件得到 `template` 后走这里；worker 从 R2/DB 读出文本、
    /// `parse_template` 得到 `template` 后走这里。核心逻辑不碰文件系统，双形态都能用。
    pub async fn plan_disable_from(
        &self,
        template: Vec<String>,
        actor: &str,
        ip: &str,
        req: &DlcDisableReq,
    ) -> AppResult<DlcDisablePlan> {
        // 豁免包：活动 requires_map_dlc（逗号分隔 → 小写集合）
        let exempt: std::collections::HashSet<String> = req
            .requires_map_dlc
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        let mut to_disable = Vec::new();
        let mut keep = Vec::new();
        for f in &template {
            let lower = f.to_lowercase();
            if exempt.contains(&lower) {
                keep.push(f.clone());
            } else {
                to_disable.push(f.clone());
            }
        }

        let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
        let backup_dir = format!("dlc_stash/{}/{}", req.game, ts);

        // 审计（disable 是客户端执行的，平台记"已下发计划"）
        self.db
            .insert_audit(&crate::domain::AuditLog {
                id: uuid::Uuid::new_v4().to_string(),
                actor: actor.into(),
                action: audit::DLC_DISABLE.into(),
                target: req.game.clone(),
                ip: ip.into(),
                detail: format!("disable={:?} keep={:?}", to_disable, keep),
                at: chrono::Utc::now(),
            })
            .await?;

        Ok(DlcDisablePlan {
            backup_dir,
            to_disable,
            keep,
        })
    }

    /// 还原审计（客户端还原后上报）
    pub async fn report_restore(&self, actor: &str, ip: &str, game: &str, restored: &[String]) -> AppResult<()> {
        self.db
            .insert_audit(&crate::domain::AuditLog {
                id: uuid::Uuid::new_v4().to_string(),
                actor: actor.into(),
                action: audit::DLC_RESTORE.into(),
                target: game.into(),
                ip: ip.into(),
                detail: format!("restored={:?}", restored),
                at: chrono::Utc::now(),
            })
            .await?;
        Ok(())
    }
}

#[allow(dead_code)]
fn _default_game() -> String { game::ETS2.into() }
