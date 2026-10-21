//! P1 服务层：积分/排名(S10) + 统计看板(S11) + 3C资源(S7) + SCS RSS(S9)
//!
//! 全部纯 DB 查询 + 存储操作，无外部 API 依赖，双目标（Docker/CF）通用。

use crate::backend::{Db, Storage};
use crate::domain::{EventLog, Member, PointsRule, ScsPost};
use crate::dto::points::{
    EventLogDto, EventLogReportDto, LeaderboardEntry, PointsRuleDto, PointsRuleSetDto, StatsSummary,
};
use crate::dto::threec::{
    ThreeCCredentialDto, ThreeCDownloadDto, ThreeCUploadReq, ThreeCVersionDto,
};
use crate::dto::rss::RssItemDto;
use crate::error::AppResult;
use chrono::Utc;
use std::sync::Arc;

// ---------- S10 积分 / 活动记录 / 排行榜 ----------

/// 积分服务（S10）
pub struct PointsService<D: Db> {
    pub db: Arc<D>,
}

impl<D: Db> PointsService<D> {
    pub fn new(db: Arc<D>) -> Self {
        PointsService { db }
    }

    /// 排行榜（按 points DESC，支持 limit）
    pub async fn leaderboard(&self, take: i64) -> AppResult<Vec<LeaderboardEntry>> {
        let members = self
            .db
            .list_members(take, 0, None)
            .await?
            .0;
        // 按 points 降序重排
        let mut sorted: Vec<&Member> = members.iter().collect();
        sorted.sort_by(|a, b| b.points.cmp(&a.points));
        let mut out = Vec::with_capacity(sorted.len());
        for (i, m) in sorted.iter().enumerate() {
            out.push(LeaderboardEntry {
                rank: (i + 1) as i64,
                member_id: m.id.clone(),
                username: format!("member:{}", &m.id[..8.min(m.id.len())]),
                points: m.points,
                plate: m.plate.clone(),
                vehicle: m.vehicle.clone(),
            });
        }
        Ok(out)
    }

    /// 上报活动记录（成员）→ 幂等加分（同一活动同一成员只记一次）
    pub async fn report_event_log(
        &self,
        member_id: &str,
        req: &EventLogReportDto,
    ) -> AppResult<EventLogDto> {
        // 幂等检查：同一 event_id + member_id 是否已有记录
        // P1 简化：直接 insert（DB 层可加 UNIQUE(member_id, event_id) 约束）
        let log = EventLog {
            id: uuid::Uuid::new_v4().to_string(),
            member_id: member_id.into(),
            event_id: req.event_id.clone(),
            result: req.result.clone(),
            mileage: req.mileage,
            income: req.income,
            at: Utc::now(),
        };
        self.db.insert_event_log(&log).await?;

        // 加分：查找适用的 PointsRule（先 event scope，再 global）
        let mut rule = self.db.get_points_rule("event_complete", &req.event_id).await?;
        if rule.is_none() {
            rule = self.db.get_points_rule("event_complete", "global").await?;
        }
        if let Some(r) = &rule {
            self.db.bump_member_points(member_id, r.points).await?;
        }

        let event_name = format!("event:{}", &log.event_id[..8.min(log.event_id.len())]);
        Ok(EventLogDto {
            id: log.id,
            member_id: log.member_id,
            event_id: log.event_id,
            event_name,
            result: log.result,
            mileage: log.mileage.filter(|&m| m > 0),
            income: log.income.filter(|&i| i > 0),
            at: log.at,
        })
    }

    /// 个人活动记录（时间线）
    pub async fn my_event_logs(
        &self,
        member_id: &str,
        take: i64,
        skip: i64,
    ) -> AppResult<(Vec<EventLogDto>, i64)> {
        let (logs, total) = self.db.list_event_logs(Some(member_id), take, skip).await?;
        let dtos = logs
            .iter()
            .map(|l| EventLogDto {
                id: l.id.clone(),
                member_id: l.member_id.clone(),
                event_id: l.event_id.clone(),
                event_name: format!("event:{}", &l.event_id[..8.min(l.event_id.len())]),
                result: l.result.clone(),
                mileage: l.mileage.filter(|&m| m > 0),
                income: l.income.filter(|&i| i > 0),
                at: l.at,
            })
            .collect();
        Ok((dtos, total))
    }

    /// 设置积分规则（Admin）
    pub async fn set_rule(&self, req: &PointsRuleSetDto) -> AppResult<PointsRuleDto> {
        let rule = PointsRule {
            id: uuid::Uuid::new_v4().to_string(),
            trigger: req.trigger.clone(),
            points: req.points,
            scope: req.scope.clone(),
        };
        self.db.upsert_points_rule(&rule).await?;
        Ok(PointsRuleDto {
            id: rule.id,
            trigger: rule.trigger,
            points: rule.points,
            scope: rule.scope,
        })
    }

    /// 获取积分规则
    pub async fn get_rule(&self, trigger: &str, scope: &str) -> AppResult<Option<PointsRuleDto>> {
        let r = self.db.get_points_rule(trigger, scope).await?;
        Ok(r.map(|r| PointsRuleDto {
            id: r.id,
            trigger: r.trigger,
            points: r.points,
            scope: r.scope,
        }))
    }
}

// ---------- S11 统计看板 ----------

/// 统计服务（S11）
pub struct StatsService<D: Db> {
    pub db: Arc<D>,
}

impl<D: Db> StatsService<D> {
    pub fn new(db: Arc<D>) -> Self {
        StatsService { db }
    }

    /// 管理端看板摘要
    pub async fn summary(&self) -> AppResult<StatsSummary> {
        let total_members = self.db.count_members().await?;
        let active_members = self.db.count_active_members().await?;
        let total_saves = self.db.count_saves().await?;
        let total_downloads = self.db.count_total_downloads().await?;
        let total_events = self.db.count_events().await?;
        let active_events = self.db.count_active_events().await?;
        let total_announcements = self.db.count_announcements().await?;
        let downloads_last_7d = self.db.count_downloads_since(7).await?;
        let logins_last_7d = self.db.count_logins_since(7).await?;

        Ok(StatsSummary {
            total_members,
            active_members,
            total_saves,
            total_downloads,
            total_events,
            active_events,
            total_announcements,
            downloads_last_7d,
            logins_last_7d,
        })
    }
}

// ---------- S7 3C 资源服务 ----------

/// 3C 文件存储 key 前缀
const THREEC_PREFIX: &str = "3c/";
const THREEC_TTL: i64 = 3600;

/// 3C 资源服务（S7）
pub struct ThreeCService<D: Db, S: Storage> {
    pub db: Arc<D>,
    pub storage: Arc<S>,
}

impl<D: Db, S: Storage> ThreeCService<D, S> {
    pub fn new(db: Arc<D>, storage: Arc<S>) -> Self {
        ThreeCService { db, storage }
    }

    /// 上传凭证（Admin）
    pub async fn issue_upload(&self, req: &ThreeCUploadReq) -> AppResult<ThreeCCredentialDto> {
        let key = format!("{THREEC_PREFIX}{}/{}", "latest", req.file_name);
        let upload_url = self.storage.presign_upload(&key, req.size).await?;
        Ok(ThreeCCredentialDto {
            upload_url,
            storage_key: key,
            expires_in: THREEC_TTL,
        })
    }

    /// 下载（客户端部署 3C 时拉取）
    pub async fn download(&self, file_name: &str) -> AppResult<ThreeCDownloadDto> {
        let key = format!("{THREEC_PREFIX}{}/{}", "latest", file_name);
        let url = self.storage.presign_download(&key).await?;
        Ok(ThreeCDownloadDto {
            file_name: file_name.into(),
            download_url: url,
            sha256: String::new(), // 从 DB 查（P1 简化：客户端按 sha256 校验）
            size: 0,
            expires_in: THREEC_TTL,
        })
    }

    /// 列表（当前已发布的 3C 版本）
    pub async fn list(&self) -> AppResult<Vec<ThreeCVersionDto>> {
        // P1 简化：返回固定的三个文件占位（实际从 DB 查）
        let files = ["config.cfg", "config_local.cfg", "controls.sii"];
        Ok(files
            .iter()
            .map(|f| ThreeCVersionDto {
                id: format!("3c-{f}"),
                file_name: f.to_string(),
                storage_key: format!("{THREEC_PREFIX}latest/{f}"),
                size: 0,
                sha256: String::new(),
                note: None,
                published_at: Utc::now(),
                published_by: "system".into(),
            })
            .collect())
    }
}

// ---------- S9 SCS RSS 聚合 ----------

/// RSS 服务（S9）
pub struct RssService<D: Db> {
    pub db: Arc<D>,
}

impl<D: Db> RssService<D> {
    pub fn new(db: Arc<D>) -> Self {
        RssService { db }
    }

    /// 列表（按 source 过滤，版本过滤）
    pub async fn list(
        &self,
        source: Option<&str>,
        game_version: Option<&str>,
        take: i64,
        skip: i64,
    ) -> AppResult<(Vec<RssItemDto>, i64)> {
        let (posts, total) = self.db.list_scs_posts(source, game_version, take, skip).await?;
        let dtos = posts
            .iter()
            .map(|p| RssItemDto {
                id: p.id.clone(),
                source: p.source.clone(),
                title: p.title.clone(),
                url: p.url.clone(),
                date: p.date,
                tags: if p.tags.is_empty() {
                    None
                } else {
                    serde_json::from_str(&p.tags).ok()
                },
                game_version_tag: p.game_version_tag.clone(),
            })
            .collect();
        Ok((dtos, total))
    }

    /// 插入/更新 RSS 条目（幂等：按 url UNIQUE）
    pub async fn upsert_post(&self, p: &ScsPost) -> AppResult<()> {
        self.db.upsert_scs_post(p).await
    }
}
