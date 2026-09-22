//! Facade: the ELPA archive installer moved to `neomacs-infra::packages`
//! (shared cache + driver seam).  Historical paths stay alive here.

pub use neomacs_infra::packages::elpa_archive::{
    GNU_ELPA_ARCHIVE, PackageArchiveSpec, prepare_cached_gnu_elpa_package,
};
