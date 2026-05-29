use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProfileResolution<T> {
    pub requested_id: String,
    pub resolved_id: String,
    pub matched: bool,
    pub fallback_used: bool,
    pub profile: T,
}

pub struct ProfileRegistry<T> {
    profiles: HashMap<String, T>,
    fallback_id: String,
}

impl<T> ProfileRegistry<T>
where
    T: Clone,
{
    pub fn new(fallback_id: &str) -> Self {
        Self {
            profiles: HashMap::new(),
            fallback_id: fallback_id.to_string(),
        }
    }

    pub fn register(&mut self, profile_id: &str, profile: T) {
        self.profiles
            .insert(profile_id.to_ascii_lowercase(), profile);
    }

    pub fn resolve(&self, profile_id: &str, fallback_name: &str) -> ProfileResolution<T> {
        let requested = profile_id.trim().to_ascii_lowercase();
        let fallback = self
            .profiles
            .get(&self.fallback_id)
            .cloned()
            .unwrap_or_else(|| panic!("{fallback_name} fallback profile must exist"));

        if requested.is_empty() {
            return ProfileResolution {
                requested_id: requested,
                resolved_id: self.fallback_id.clone(),
                matched: false,
                fallback_used: true,
                profile: fallback,
            };
        }

        if let Some(profile) = self.profiles.get(&requested).cloned() {
            return ProfileResolution {
                requested_id: requested.clone(),
                resolved_id: requested,
                matched: true,
                fallback_used: false,
                profile,
            };
        }

        ProfileResolution {
            requested_id: requested,
            resolved_id: self.fallback_id.clone(),
            matched: false,
            fallback_used: true,
            profile: fallback,
        }
    }
}
