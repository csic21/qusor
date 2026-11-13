//! Native page image returned to the trusted TSX surface.

use nsh_core::{
    ImageData,
    element::{JsHost, JsIn, JsOwned, JsValue, PropError, TsTypes},
};

pub(crate) struct PdfPageData {
    pub info: String,
    pub image: Option<ImageData>,
}

pub struct PdfPageFile(PdfPageData);

impl From<PdfPageData> for PdfPageFile {
    fn from(value: PdfPageData) -> Self {
        Self(value)
    }
}

impl JsValue for PdfPageFile {
    fn ts_type(types: &mut TsTypes) -> String {
        types.declare("LibraryPdfPage", |_| {
            "export interface LibraryPdfPage { info: string; file: File | null; }".into()
        })
    }

    fn from_js(value: JsIn<'_>) -> Result<Self, PropError> {
        Err(value.expected("native PDF page output only"))
    }

    fn to_js<'a>(&self, host: &'a dyn JsHost) -> Result<JsOwned<'a>, PropError> {
        let owned = JsOwned::new(host, host.new_object()?);
        let object = owned.get().raw();
        host.set_field(object, "info", host.new_string(&self.0.info)?)?;
        let file = match &self.0.image {
            Some(image) => host.new_file(image, "image/png", "qusor-page.png", None)?,
            None => host.null(),
        };
        host.set_field(object, "file", file)?;
        Ok(owned)
    }
}

pub(crate) fn failed_page(error: String) -> PdfPageFile {
    PdfPageData {
        info: serde_json::json!({"ok": false, "error": error}).to_string(),
        image: None,
    }
    .into()
}
