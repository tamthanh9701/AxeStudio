use crate::error::{OrchError, Result};
use als_core::ProviderId;
use als_provider::RenderProvider;
use std::collections::HashMap;
use std::sync::Arc;

/// Sổ đăng ký provider. Orchestrator chỉ biết trait — không biết đang nói
/// chuyện với cpp, py hay mock (ADR-001).
pub struct Registry {
    providers: HashMap<String, Arc<dyn RenderProvider>>,
    active: ProviderId,
}

impl Registry {
    pub fn new(providers: Vec<Arc<dyn RenderProvider>>, active: ProviderId) -> Result<Self> {
        let map: HashMap<String, Arc<dyn RenderProvider>> = providers
            .into_iter()
            .map(|p| (p.id().0.clone(), p))
            .collect();
        if !map.contains_key(active.as_str()) {
            return Err(OrchError::NoProvider(active.to_string()));
        }
        Ok(Self {
            providers: map,
            active,
        })
    }

    pub fn active_id(&self) -> ProviderId {
        self.active.clone()
    }

    pub fn active_provider(&self) -> Arc<dyn RenderProvider> {
        self.providers
            .get(self.active.as_str())
            .expect("active provider tồn tại — Registry::new đã kiểm")
            .clone()
    }

    pub fn set_active(&mut self, id: ProviderId) -> Result<()> {
        if !self.providers.contains_key(id.as_str()) {
            return Err(OrchError::NoProvider(id.to_string()));
        }
        self.active = id;
        Ok(())
    }

    pub fn list(&self) -> Vec<ProviderId> {
        let mut v: Vec<_> = self.providers.keys().map(|k| ProviderId(k.clone())).collect();
        v.sort();
        v
    }
}
