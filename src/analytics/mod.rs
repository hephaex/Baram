//! Analytics module for trend analysis and insights
//! Issue #8: Trend Analysis & Notification Features

pub mod keyword_trends;
pub mod entity_trends;

pub use keyword_trends::{KeywordTrend, TrendAnalyzer, TrendDirection, DataPoint, Spike, TrendError};
pub use entity_trends::{Entity, EntityNetwork, Cooccurrence, EntityType, EntityMention, EntityError};
