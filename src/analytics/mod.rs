//! Analytics module for trend analysis and insights
//! Issue #8: Trend Analysis & Notification Features

pub mod entity_trends;
pub mod keyword_trends;

pub use entity_trends::{
    Cooccurrence, Entity, EntityError, EntityMention, EntityNetwork, EntityType,
};
pub use keyword_trends::{
    DataPoint, KeywordTrend, Spike, TrendAnalyzer, TrendDirection, TrendError,
};
