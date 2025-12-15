//! Category assignment logic
//!
//! This module handles the assignment of news categories to crawler instances,
//! including priority weighting, load balancing, and dynamic rebalancing.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::rotation::{CrawlerInstance, NewsCategory};
use super::schedule::{DailySchedule, HourlySlot};

// ============================================================================
// Category Priority
// ============================================================================

/// Priority level for category crawling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CategoryPriority {
    /// Low priority - crawl less frequently
    Low = 1,
    /// Normal priority - standard crawl frequency
    Normal = 2,
    /// High priority - crawl more frequently
    High = 3,
    /// Critical priority - always crawl
    Critical = 4,
}

impl Default for CategoryPriority {
    fn default() -> Self {
        Self::Normal
    }
}

impl CategoryPriority {
    /// Get weight multiplier for this priority
    pub fn weight(&self) -> f64 {
        match self {
            Self::Low => 0.5,
            Self::Normal => 1.0,
            Self::High => 1.5,
            Self::Critical => 2.0,
        }
    }

    /// Get slots per day multiplier
    pub fn slots_multiplier(&self) -> usize {
        match self {
            Self::Low => 1,
            Self::Normal => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

// ============================================================================
// Category Configuration
// ============================================================================

/// Configuration for a single category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryConfig {
    /// The category
    pub category: NewsCategory,

    /// Priority level
    pub priority: CategoryPriority,

    /// Whether this category is enabled
    pub enabled: bool,

    /// Preferred instances (empty means all)
    pub preferred_instances: Vec<CrawlerInstance>,

    /// Excluded instances (will not be assigned to these)
    pub excluded_instances: Vec<CrawlerInstance>,

    /// Maximum articles per crawl session
    pub max_articles: Option<u32>,

    /// Minimum hours between crawls
    pub min_interval_hours: u8,
}

impl CategoryConfig {
    /// Create a new category config with defaults
    pub fn new(category: NewsCategory) -> Self {
        Self {
            category,
            priority: CategoryPriority::Normal,
            enabled: true,
            preferred_instances: Vec::new(),
            excluded_instances: Vec::new(),
            max_articles: None,
            min_interval_hours: 1,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: CategoryPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set enabled status
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Add preferred instance
    pub fn with_preferred_instance(mut self, instance: CrawlerInstance) -> Self {
        if !self.preferred_instances.contains(&instance) {
            self.preferred_instances.push(instance);
        }
        self
    }

    /// Add excluded instance
    pub fn with_excluded_instance(mut self, instance: CrawlerInstance) -> Self {
        if !self.excluded_instances.contains(&instance) {
            self.excluded_instances.push(instance);
        }
        self
    }

    /// Set max articles
    pub fn with_max_articles(mut self, max: u32) -> Self {
        self.max_articles = Some(max);
        self
    }

    /// Check if instance can handle this category
    pub fn can_assign_to(&self, instance: CrawlerInstance) -> bool {
        if !self.enabled {
            return false;
        }
        if self.excluded_instances.contains(&instance) {
            return false;
        }
        if !self.preferred_instances.is_empty() && !self.preferred_instances.contains(&instance) {
            return false;
        }
        true
    }
}

// ============================================================================
// Assignment Strategy
// ============================================================================

/// Strategy for assigning categories to instances
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentStrategy {
    /// Round-robin assignment
    RoundRobin,
    /// Priority-weighted assignment
    Weighted,
    /// Load-balanced assignment
    LoadBalanced,
    /// Affinity-based (prefer same instance for same category)
    Affinity,
}

impl Default for AssignmentStrategy {
    fn default() -> Self {
        Self::RoundRobin
    }
}

// ============================================================================
// Category Assigner
// ============================================================================

/// Handles category-to-instance assignments
pub struct CategoryAssigner {
    /// Category configurations
    configs: HashMap<NewsCategory, CategoryConfig>,

    /// Assignment strategy
    strategy: AssignmentStrategy,

    /// Categories per slot
    categories_per_slot: usize,

    /// Instance load tracking (for load balancing)
    instance_loads: HashMap<CrawlerInstance, usize>,

    /// Category affinity tracking (for affinity strategy)
    category_affinity: HashMap<NewsCategory, CrawlerInstance>,
}

impl CategoryAssigner {
    /// Create a new category assigner with default configs
    pub fn new() -> Self {
        let mut configs = HashMap::new();
        for category in NewsCategory::all() {
            configs.insert(category, CategoryConfig::new(category));
        }

        Self {
            configs,
            strategy: AssignmentStrategy::RoundRobin,
            categories_per_slot: 2,
            instance_loads: HashMap::new(),
            category_affinity: HashMap::new(),
        }
    }

    /// Set assignment strategy
    pub fn with_strategy(mut self, strategy: AssignmentStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set categories per slot
    pub fn with_categories_per_slot(mut self, count: usize) -> Self {
        self.categories_per_slot = count.max(1).min(NewsCategory::all().len());
        self
    }

    /// Update configuration for a category
    pub fn configure_category(&mut self, config: CategoryConfig) {
        self.configs.insert(config.category, config);
    }

    /// Set priority for a category
    pub fn set_priority(&mut self, category: NewsCategory, priority: CategoryPriority) {
        if let Some(config) = self.configs.get_mut(&category) {
            config.priority = priority;
        }
    }

    /// Enable or disable a category
    pub fn set_enabled(&mut self, category: NewsCategory, enabled: bool) {
        if let Some(config) = self.configs.get_mut(&category) {
            config.enabled = enabled;
        }
    }

    /// Get enabled categories
    pub fn enabled_categories(&self) -> Vec<NewsCategory> {
        self.configs
            .values()
            .filter(|c| c.enabled)
            .map(|c| c.category)
            .collect()
    }

    /// Get categories sorted by priority (highest first)
    pub fn categories_by_priority(&self) -> Vec<NewsCategory> {
        let mut cats: Vec<_> = self.enabled_categories();
        cats.sort_by(|a, b| {
            let pa = self.configs.get(a).map(|c| c.priority).unwrap_or_default();
            let pb = self.configs.get(b).map(|c| c.priority).unwrap_or_default();
            pb.cmp(&pa)
        });
        cats
    }

    /// Assign categories to a slot
    pub fn assign_categories_to_slot(
        &mut self,
        hour: u8,
        instance: CrawlerInstance,
    ) -> Vec<NewsCategory> {
        match self.strategy {
            AssignmentStrategy::RoundRobin => self.assign_round_robin(hour),
            AssignmentStrategy::Weighted => self.assign_weighted(hour, instance),
            AssignmentStrategy::LoadBalanced => self.assign_load_balanced(hour, instance),
            AssignmentStrategy::Affinity => self.assign_affinity(hour, instance),
        }
    }

    /// Round-robin assignment (cyclic)
    fn assign_round_robin(&self, hour: u8) -> Vec<NewsCategory> {
        let enabled = self.enabled_categories();
        if enabled.is_empty() {
            return Vec::new();
        }

        let start = (hour as usize * self.categories_per_slot) % enabled.len();

        enabled
            .iter()
            .cycle()
            .skip(start)
            .take(self.categories_per_slot)
            .copied()
            .collect()
    }

    /// Priority-weighted assignment
    fn assign_weighted(&self, hour: u8, instance: CrawlerInstance) -> Vec<NewsCategory> {
        let mut categories: Vec<_> = self
            .configs
            .values()
            .filter(|c| c.enabled && c.can_assign_to(instance))
            .collect();

        // Sort by priority (highest first)
        categories.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Calculate how many of each priority to include
        let mut result = Vec::new();
        let mut remaining = self.categories_per_slot;

        for config in categories {
            if remaining == 0 {
                break;
            }

            // Higher priority categories get more slots
            let slots = (config.priority.slots_multiplier()).min(remaining);

            // Use hour-based offset to rotate through categories
            let offset = hour as usize;
            if (offset % 4) < slots {
                result.push(config.category);
                remaining -= 1;
            }
        }

        // Fill remaining with round-robin if needed
        if result.len() < self.categories_per_slot {
            let rr = self.assign_round_robin(hour);
            for cat in rr {
                if !result.contains(&cat) && result.len() < self.categories_per_slot {
                    result.push(cat);
                }
            }
        }

        result
    }

    /// Load-balanced assignment
    fn assign_load_balanced(&mut self, hour: u8, instance: CrawlerInstance) -> Vec<NewsCategory> {
        // Track instance load
        *self.instance_loads.entry(instance).or_insert(0) += 1;

        // Get categories this instance can handle
        let mut available: Vec<_> = self
            .configs
            .values()
            .filter(|c| c.enabled && c.can_assign_to(instance))
            .map(|c| c.category)
            .collect();

        if available.is_empty() {
            return self.assign_round_robin(hour);
        }

        // Sort by how recently they were assigned (simple rotation)
        let load = *self.instance_loads.get(&instance).unwrap_or(&0);
        let len = available.len();
        let offset = (hour as usize + load) % len;

        available.rotate_left(offset);
        available.truncate(self.categories_per_slot);
        available
    }

    /// Affinity-based assignment (prefer same instance for same category)
    fn assign_affinity(&mut self, hour: u8, instance: CrawlerInstance) -> Vec<NewsCategory> {
        let mut result = Vec::new();

        // First, add categories with affinity to this instance
        for (category, affine_instance) in &self.category_affinity {
            if *affine_instance == instance && result.len() < self.categories_per_slot {
                if let Some(config) = self.configs.get(category) {
                    if config.enabled && config.can_assign_to(instance) {
                        result.push(*category);
                    }
                }
            }
        }

        // Fill remaining slots with round-robin
        if result.len() < self.categories_per_slot {
            let rr = self.assign_round_robin(hour);
            for cat in rr {
                if !result.contains(&cat) && result.len() < self.categories_per_slot {
                    result.push(cat);
                    // Set affinity
                    self.category_affinity.insert(cat, instance);
                }
            }
        }

        result
    }

    /// Generate a complete daily schedule with category assignments
    pub fn generate_schedule(
        &mut self,
        date: NaiveDate,
        instance_rotation: &[CrawlerInstance],
    ) -> DailySchedule {
        let mut slots = Vec::with_capacity(24);

        for hour in 0..24 {
            let instance = instance_rotation[hour % instance_rotation.len()];
            let categories = self.assign_categories_to_slot(hour as u8, instance);

            slots.push(HourlySlot {
                hour: hour as u8,
                instance,
                categories,
            });
        }

        DailySchedule::new(date, slots)
    }

    /// Rebalance categories across instances
    pub fn rebalance(&mut self, schedule: &mut DailySchedule) {
        // Count category occurrences per instance
        let mut instance_category_counts: HashMap<CrawlerInstance, HashMap<NewsCategory, usize>> =
            HashMap::new();

        for slot in &schedule.slots {
            let counts = instance_category_counts.entry(slot.instance).or_default();
            for cat in &slot.categories {
                *counts.entry(*cat).or_insert(0) += 1;
            }
        }

        // Calculate average and identify imbalances
        let total_categories: usize = schedule.slots.iter().map(|s| s.categories.len()).sum();
        let avg_per_instance = total_categories / CrawlerInstance::count();

        // Find over and under-assigned instances
        let mut adjustments_needed = false;
        for (instance, counts) in &instance_category_counts {
            let total: usize = counts.values().sum();
            if total > avg_per_instance + 2 || total < avg_per_instance.saturating_sub(2) {
                adjustments_needed = true;
                tracing::debug!(
                    "Instance {} has {} categories (avg: {})",
                    instance,
                    total,
                    avg_per_instance
                );
            }
        }

        if adjustments_needed {
            tracing::info!("Rebalancing needed but not yet implemented");
            // Future: Implement actual rebalancing logic
        }
    }

    /// Get assignment statistics
    pub fn stats(&self) -> AssignmentStats {
        let enabled_count = self.enabled_categories().len();
        let priority_dist: HashMap<CategoryPriority, usize> = self
            .configs
            .values()
            .filter(|c| c.enabled)
            .fold(HashMap::new(), |mut acc, c| {
                *acc.entry(c.priority).or_insert(0) += 1;
                acc
            });

        AssignmentStats {
            total_categories: NewsCategory::all().len(),
            enabled_categories: enabled_count,
            categories_per_slot: self.categories_per_slot,
            strategy: self.strategy,
            priority_distribution: priority_dist,
        }
    }

    /// Reset load tracking
    pub fn reset_loads(&mut self) {
        self.instance_loads.clear();
    }

    /// Clear affinity mappings
    pub fn clear_affinity(&mut self) {
        self.category_affinity.clear();
    }
}

impl Default for CategoryAssigner {
    fn default() -> Self {
        Self::new()
    }
}

/// Assignment statistics
#[derive(Debug, Clone)]
pub struct AssignmentStats {
    pub total_categories: usize,
    pub enabled_categories: usize,
    pub categories_per_slot: usize,
    pub strategy: AssignmentStrategy,
    pub priority_distribution: HashMap<CategoryPriority, usize>,
}

impl AssignmentStats {
    /// Format as display string
    pub fn display(&self) -> String {
        let mut output = String::from("Assignment Statistics\n");
        output.push_str(&format!("{:-<40}\n", ""));
        output.push_str(&format!(
            "Categories: {}/{} enabled\n",
            self.enabled_categories, self.total_categories
        ));
        output.push_str(&format!("Per Slot: {}\n", self.categories_per_slot));
        output.push_str(&format!("Strategy: {:?}\n", self.strategy));
        output.push_str("\nPriority Distribution:\n");
        for (priority, count) in &self.priority_distribution {
            output.push_str(&format!("  {:?}: {}\n", priority, count));
        }
        output
    }
}

// ============================================================================
// Instance Affinity Map
// ============================================================================

/// Maps categories to preferred instances with weights
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AffinityMap {
    /// Affinity weights: (category, instance) -> weight
    weights: HashMap<(NewsCategory, CrawlerInstance), f64>,
}

impl AffinityMap {
    /// Create a new affinity map
    pub fn new() -> Self {
        Self::default()
    }

    /// Set affinity weight
    pub fn set_affinity(&mut self, category: NewsCategory, instance: CrawlerInstance, weight: f64) {
        self.weights.insert((category, instance), weight.clamp(0.0, 1.0));
    }

    /// Get affinity weight
    pub fn get_affinity(&self, category: NewsCategory, instance: CrawlerInstance) -> f64 {
        *self.weights.get(&(category, instance)).unwrap_or(&0.5)
    }

    /// Get best instance for a category
    pub fn best_instance_for(&self, category: NewsCategory) -> Option<CrawlerInstance> {
        let mut best: Option<(CrawlerInstance, f64)> = None;

        for instance in CrawlerInstance::all() {
            let weight = self.get_affinity(category, instance);
            if best.is_none() || weight > best.unwrap().1 {
                best = Some((instance, weight));
            }
        }

        best.map(|(i, _)| i)
    }

    /// Learn from successful crawl (increase affinity)
    pub fn record_success(&mut self, category: NewsCategory, instance: CrawlerInstance) {
        let current = self.get_affinity(category, instance);
        self.set_affinity(category, instance, current + 0.1);
    }

    /// Learn from failed crawl (decrease affinity)
    pub fn record_failure(&mut self, category: NewsCategory, instance: CrawlerInstance) {
        let current = self.get_affinity(category, instance);
        self.set_affinity(category, instance, current - 0.1);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_priority_weight() {
        assert!(CategoryPriority::Low.weight() < CategoryPriority::Normal.weight());
        assert!(CategoryPriority::Normal.weight() < CategoryPriority::High.weight());
        assert!(CategoryPriority::High.weight() < CategoryPriority::Critical.weight());
    }

    #[test]
    fn test_category_config() {
        let config = CategoryConfig::new(NewsCategory::Politics)
            .with_priority(CategoryPriority::High)
            .with_preferred_instance(CrawlerInstance::Main)
            .with_max_articles(100);

        assert_eq!(config.priority, CategoryPriority::High);
        assert!(config.can_assign_to(CrawlerInstance::Main));
        assert!(!config.can_assign_to(CrawlerInstance::Sub1));
        assert_eq!(config.max_articles, Some(100));
    }

    #[test]
    fn test_category_config_exclusion() {
        let config = CategoryConfig::new(NewsCategory::Economy)
            .with_excluded_instance(CrawlerInstance::Sub2);

        assert!(config.can_assign_to(CrawlerInstance::Main));
        assert!(config.can_assign_to(CrawlerInstance::Sub1));
        assert!(!config.can_assign_to(CrawlerInstance::Sub2));
    }

    #[test]
    fn test_category_assigner_creation() {
        let assigner = CategoryAssigner::new();

        // All categories should be enabled by default
        assert_eq!(assigner.enabled_categories().len(), NewsCategory::all().len());
    }

    #[test]
    fn test_category_assigner_round_robin() {
        let assigner = CategoryAssigner::new().with_categories_per_slot(2);

        let cats_h0 = assigner.assign_round_robin(0);
        let cats_h1 = assigner.assign_round_robin(1);

        assert_eq!(cats_h0.len(), 2);
        assert_eq!(cats_h1.len(), 2);

        // Different hours should have different starting categories
        // (due to rotation)
    }

    #[test]
    fn test_category_assigner_set_priority() {
        let mut assigner = CategoryAssigner::new();
        assigner.set_priority(NewsCategory::Politics, CategoryPriority::Critical);

        let by_priority = assigner.categories_by_priority();
        assert_eq!(by_priority[0], NewsCategory::Politics);
    }

    #[test]
    fn test_category_assigner_disable() {
        let mut assigner = CategoryAssigner::new();

        let before = assigner.enabled_categories().len();
        assigner.set_enabled(NewsCategory::Culture, false);
        let after = assigner.enabled_categories().len();

        assert_eq!(after, before - 1);
        assert!(!assigner.enabled_categories().contains(&NewsCategory::Culture));
    }

    #[test]
    fn test_category_assigner_generate_schedule() {
        let mut assigner = CategoryAssigner::new().with_categories_per_slot(2);
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let rotation = CrawlerInstance::all();

        let schedule = assigner.generate_schedule(date, &rotation);

        assert_eq!(schedule.slots.len(), 24);
        for slot in &schedule.slots {
            assert_eq!(slot.categories.len(), 2);
        }
    }

    #[test]
    fn test_assignment_stats() {
        let assigner = CategoryAssigner::new();
        let stats = assigner.stats();

        assert_eq!(stats.total_categories, 6);
        assert_eq!(stats.enabled_categories, 6);
        assert_eq!(stats.strategy, AssignmentStrategy::RoundRobin);
    }

    #[test]
    fn test_affinity_map() {
        let mut affinity = AffinityMap::new();

        affinity.set_affinity(NewsCategory::Politics, CrawlerInstance::Main, 0.9);
        affinity.set_affinity(NewsCategory::Politics, CrawlerInstance::Sub1, 0.3);

        assert_eq!(affinity.best_instance_for(NewsCategory::Politics), Some(CrawlerInstance::Main));
    }

    #[test]
    fn test_affinity_map_learning() {
        let mut affinity = AffinityMap::new();

        let initial = affinity.get_affinity(NewsCategory::Economy, CrawlerInstance::Sub1);

        affinity.record_success(NewsCategory::Economy, CrawlerInstance::Sub1);
        let after_success = affinity.get_affinity(NewsCategory::Economy, CrawlerInstance::Sub1);
        assert!(after_success > initial);

        affinity.record_failure(NewsCategory::Economy, CrawlerInstance::Sub1);
        let after_failure = affinity.get_affinity(NewsCategory::Economy, CrawlerInstance::Sub1);
        assert!(after_failure < after_success);
    }

    #[test]
    fn test_assignment_strategy_weighted() {
        let mut assigner = CategoryAssigner::new()
            .with_strategy(AssignmentStrategy::Weighted)
            .with_categories_per_slot(3);

        // Set high priority for politics
        assigner.set_priority(NewsCategory::Politics, CategoryPriority::Critical);

        let cats = assigner.assign_categories_to_slot(0, CrawlerInstance::Main);
        assert_eq!(cats.len(), 3);
    }
}
