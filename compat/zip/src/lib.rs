//! Compatibility adapter for umya-spreadsheet's zip 8 requirement.
//! All implementation is provided by zip 9; no withdrawn zip code is used.
pub use zip_next::*;

pub use read::ZipArchive;
pub use write::ZipWriter;

pub mod write {
    use std::io::{Read, Seek, Write};
    use std::ops::{Deref, DerefMut};
    use zip_next::result::ZipResult;
    pub use zip_next::write::*;

    pub struct ZipWriter<W: Write + Seek>(zip_next::ZipWriter<W>);

    impl<W: Write + Seek> ZipWriter<W> {
        pub fn new(writer: W) -> Self {
            Self(zip_next::ZipWriter::new(writer))
        }
        pub fn finish(self) -> ZipResult<W> {
            self.0.finish()
        }
        pub fn raw_copy_file_rename<R: Read, S: ToString>(
            &mut self,
            file: super::read::ZipFile<'_, R>,
            name: S,
        ) -> ZipResult<()> {
            self.0.raw_copy_file_rename(file.inner, name)
        }
    }

    impl<W: Write + Seek> Deref for ZipWriter<W> {
        type Target = zip_next::ZipWriter<W>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<W: Write + Seek> DerefMut for ZipWriter<W> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<W: Write + Seek> Write for ZipWriter<W> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }
}

pub mod read {
    use std::io::{Read, Seek};
    use std::ops::{Deref, DerefMut};
    pub use zip_next::read::*;
    use zip_next::result::ZipResult;

    /// zip 9 validates entry names fallibly; umya expects zip 8's name API.
    /// Validate while opening the entry, so malformed names still propagate.
    pub struct ZipFile<'a, R: Read> {
        pub(super) inner: zip_next::read::ZipFile<'a, R>,
        name: String,
    }

    impl<'a, R: Read> ZipFile<'a, R> {
        fn wrap(inner: zip_next::read::ZipFile<'a, R>) -> ZipResult<Self> {
            let name = inner.name()?.into_owned();
            Ok(Self { inner, name })
        }

        pub fn name(&self) -> &str {
            &self.name
        }
    }

    impl<'a, R: Read> Deref for ZipFile<'a, R> {
        type Target = zip_next::read::ZipFile<'a, R>;
        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl<R: Read> Read for ZipFile<'_, R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.inner.read(buf)
        }
    }

    pub struct ZipArchive<R>(zip_next::ZipArchive<R>);

    impl<R: Read + Seek> ZipArchive<R> {
        pub fn new(reader: R) -> ZipResult<Self> {
            zip_next::ZipArchive::new(reader).map(Self)
        }
        pub fn by_name(&mut self, name: &str) -> ZipResult<ZipFile<'_, R>> {
            ZipFile::wrap(self.0.by_name(name)?)
        }
        pub fn by_index(&mut self, index: usize) -> ZipResult<ZipFile<'_, R>> {
            ZipFile::wrap(self.0.by_index(index)?)
        }
    }

    impl<R> Deref for ZipArchive<R> {
        type Target = zip_next::ZipArchive<R>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<R> DerefMut for ZipArchive<R> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
}
