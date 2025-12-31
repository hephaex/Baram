pub mod crawl;
pub mod index;
pub mod ontology;
pub mod search;
pub mod serve;

// Re-export command functions for convenience
pub use crawl::{crawl, resume, stats};
pub use index::index;
pub use ontology::ontology;
pub use search::search;
pub use serve::{
    coordinator_server, distributed_crawler, embedding_server, CoordinatorParams,
    DistributedCrawlerParams,
};
