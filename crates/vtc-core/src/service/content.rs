//! 公告 / 活动服务（S8）

use crate::backend::Backends;
use crate::constants::{audit, status};
use crate::domain::{Announcement, Event};
use crate::dto::content::{
    AnnouncementCreateDto, AnnouncementDto, AnnouncementUpdateDto, EventCreateDto, EventDto, EventUpdateDto,
};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use std::sync::Arc;

pub struct ContentService {
    pub backends: Backends,
    pub jwt: Arc<crate::auth::JwtService>,
}

impl ContentService {
    pub fn new(backends: Backends, jwt: Arc<crate::auth::JwtService>) -> Self {
        ContentService { backends, jwt }
    }

    // ---------- 公告 ----------

    fn ann_dto(a: &Announcement) -> AnnouncementDto {
        AnnouncementDto {
            id: a.id.clone(),
            title: a.title.clone(),
            body: a.body.clone(),
            status: a.status.clone(),
            published_at: a.published_at,
            scheduled_at: a.scheduled_at,
            pinned: a.pinned,
            author_id: a.author_id.clone(),
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }

    /// 列表（客户端）
    pub async fn list_announcements(&self, status_: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<AnnouncementDto>, i64)> {
        let (anns, total) = self.backends.db.list_announcements(status_, take, skip).await?;
        Ok((anns.iter().map(Self::ann_dto).collect(), total))
    }

    pub async fn get_announcement(&self, id: &str) -> AppResult<AnnouncementDto> {
        let a = self.backends.db.get_announcement(id).await?
            .ok_or_else(|| AppError::NotFound("公告不存在".into()))?;
        Ok(Self::ann_dto(&a))
    }

    pub async fn create_announcement(&self, req: &AnnouncementCreateDto, actor: &str, ip: &str) -> AppResult<AnnouncementDto> {
        let now = Utc::now();
        let published_at = if req.status == status::PUBLISHED { Some(now) } else { None };
        let a = Announcement {
            id: uuid::Uuid::new_v4().to_string(),
            title: req.title.clone(),
            body: req.body.clone(),
            status: req.status.clone(),
            published_at,
            scheduled_at: req.scheduled_at,
            pinned: req.pinned,
            author_id: Some(actor.into()),
            created_at: now,
            updated_at: now,
        };
        self.backends.db.insert_announcement(&a).await?;
        self.audit(actor, audit::ANNOUNCE_CREATE, ip, &a.id).await;
        Ok(Self::ann_dto(&a))
    }

    pub async fn update_announcement(&self, id: &str, req: &AnnouncementUpdateDto, actor: &str, ip: &str) -> AppResult<AnnouncementDto> {
        let mut a = self.backends.db.get_announcement(id).await?
            .ok_or_else(|| AppError::NotFound("公告不存在".into()))?;
        if let Some(t) = &req.title { a.title = t.clone(); }
        if let Some(b) = &req.body { a.body = b.clone(); }
        if let Some(s) = &req.status { a.status = s.clone(); }
        if let Some(p) = req.pinned { a.pinned = p; }
        if let Some(sa) = req.scheduled_at { a.scheduled_at = Some(sa); }
        a.updated_at = Utc::now();
        self.backends.db.update_announcement(&a).await?;
        self.audit(actor, "announcement.update", ip, id).await;
        Ok(Self::ann_dto(&a))
    }

    pub async fn delete_announcement(&self, id: &str, actor: &str, ip: &str) -> AppResult<()> {
        self.backends.db.delete_announcement(id).await?;
        self.audit(actor, "announcement.delete", ip, id).await;
        Ok(())
    }

    // ---------- 活动 ----------

    fn evt_dto(e: &Event) -> EventDto {
        EventDto {
            id: e.id.clone(),
            name: e.name.clone(),
            etype: e.etype.clone(),
            activity_at: e.activity_at,
            depart_at: e.depart_at,
            server: e.server.clone(),
            navigation: e.navigation.clone(),
            image_key: e.image_key.clone(),
            save_id: e.save_id.clone(),
            status: e.status.clone(),
            requires_map_dlc: e.requires_map_dlc.clone(),
            created_by: e.created_by.clone(),
        }
    }

    pub async fn list_events(&self, status_: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<EventDto>, i64)> {
        let (evts, total) = self.backends.db.list_events(status_, take, skip).await?;
        Ok((evts.iter().map(Self::evt_dto).collect(), total))
    }

    pub async fn get_event(&self, id: &str) -> AppResult<EventDto> {
        let e = self.backends.db.get_event(id).await?
            .ok_or_else(|| AppError::NotFound("活动不存在".into()))?;
        Ok(Self::evt_dto(&e))
    }

    pub async fn create_event(&self, req: &EventCreateDto, actor: &str, ip: &str) -> AppResult<EventDto> {
        let e = Event {
            id: uuid::Uuid::new_v4().to_string(),
            name: req.name.clone(),
            etype: req.etype.clone(),
            activity_at: req.activity_at,
            depart_at: req.depart_at,
            server: req.server.clone(),
            navigation: req.navigation.clone(),
            image_key: req.image_key.clone(),
            save_id: req.save_id.clone(),
            status: req.status.clone(),
            requires_map_dlc: req.requires_map_dlc.clone(),
            created_by: Some(actor.into()),
        };
        self.backends.db.insert_event(&e).await?;
        self.audit(actor, audit::EVENT_CREATE, ip, &e.id).await;
        Ok(Self::evt_dto(&e))
    }

    pub async fn update_event(&self, id: &str, req: &EventUpdateDto, actor: &str, ip: &str) -> AppResult<EventDto> {
        let mut e = self.backends.db.get_event(id).await?
            .ok_or_else(|| AppError::NotFound("活动不存在".into()))?;
        if let Some(n) = &req.name { e.name = n.clone(); }
        if let Some(et) = &req.etype { e.etype = et.clone(); }
        if let Some(a) = req.activity_at { e.activity_at = a; }
        if let Some(d) = req.depart_at { e.depart_at = Some(d); }
        if let Some(s) = &req.server { e.server = s.clone(); }
        if let Some(nv) = &req.navigation { e.navigation = nv.clone(); }
        if let Some(ik) = &req.image_key { e.image_key = Some(ik.clone()); }
        if let Some(sid) = &req.save_id { e.save_id = Some(sid.clone()); }
        if let Some(st) = &req.status { e.status = st.clone(); }
        if let Some(r) = &req.requires_map_dlc { e.requires_map_dlc = Some(r.clone()); }
        self.backends.db.update_event(&e).await?;
        self.audit(actor, "event.update", ip, id).await;
        Ok(Self::evt_dto(&e))
    }

    pub async fn delete_event(&self, id: &str, actor: &str, ip: &str) -> AppResult<()> {
        self.backends.db.delete_event(id).await?;
        self.audit(actor, "event.delete", ip, id).await;
        Ok(())
    }

    async fn audit(&self, actor: &str, action: &str, ip: &str, target: &str) {
        if let Err(e) = self
            .backends
            .db
            .insert_audit(&crate::domain::AuditLog {
                id: uuid::Uuid::new_v4().to_string(),
                actor: actor.into(),
                action: action.into(),
                target: target.into(),
                ip: ip.into(),
                detail: String::new(),
                at: Utc::now(),
            })
            .await
        {
            tracing::warn!(err = %e, "审计写入失败");
        }
    }
}
