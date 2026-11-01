pub mod dependency;

pub use dependency::{
    Dependency, DependencyFeature, DependencyGroup, DependencyNode, DependencyTree,
    DependencyUsage, NodeId, PackageFormatMetadata, PackageFormatRequirements, ResolutionOptions,
};
