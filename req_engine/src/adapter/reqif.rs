//! ReqIF adapter using PyO3 and the reqif Python library.
//!
//! This adapter provides import/export functionality for ReqIF files,
//! enabling interoperability with DOORS, Polarion, and other tools.
//!
//! Requires the `reqif` feature and Python with the `reqif` package installed:
//! ```bash
//! pip install reqif
//! cargo build --features reqif
//! ```

use crate::adapter::RequirementAdapter;
use crate::{Error, Result};
use req_lib::{Requirement, RequirementStatus, RequirementType};
use std::collections::HashMap;
use std::path::Path;

/// ReqIF attribute mapping configuration.
///
/// Defines how ReqIF attribute names map to internal requirement fields.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReqIfMapping {
    /// Attribute name for requirement ID
    pub id_attribute: String,
    /// Attribute name for requirement text
    pub text_attribute: String,
    /// Attribute name for requirement title
    pub title_attribute: Option<String>,
    /// Attribute name for status
    pub status_attribute: Option<String>,
    /// Attribute name for parent reference
    pub parent_attribute: Option<String>,
    /// Mapping from ReqIF status values to internal status
    #[serde(default)]
    pub status_mapping: HashMap<String, String>,
    /// Type name for HLR requirements
    pub hlr_type: Option<String>,
    /// Type name for LLR requirements
    pub llr_type: Option<String>,
    /// Type name for Test requirements
    pub tst_type: Option<String>,
}

impl Default for ReqIfMapping {
    fn default() -> Self {
        let mut status_mapping = HashMap::new();
        status_mapping.insert("approved".to_string(), "approved".to_string());
        status_mapping.insert("draft".to_string(), "draft".to_string());
        status_mapping.insert("deprecated".to_string(), "deprecated".to_string());
        status_mapping.insert("rejected".to_string(), "rejected".to_string());

        Self {
            id_attribute: "ReqIF.ForeignID".to_string(),
            text_attribute: "ReqIF.Content".to_string(),
            title_attribute: Some("ReqIF.Name".to_string()),
            status_attribute: Some("ReqIF.Status".to_string()),
            parent_attribute: None,
            status_mapping,
            hlr_type: Some("HLR".to_string()),
            llr_type: Some("LLR".to_string()),
            tst_type: Some("TST".to_string()),
        }
    }
}

/// ReqIF adapter for importing and exporting requirements
pub struct ReqIfAdapter {
    mapping: ReqIfMapping,
}

impl ReqIfAdapter {
    // REQ: LLR-0009
    /// Create with default mapping
    pub fn new() -> Self {
        Self {
            mapping: ReqIfMapping::default(),
        }
    }

    /// Create with custom mapping
    pub fn with_mapping(mapping: ReqIfMapping) -> Self {
        Self { mapping }
    }

    #[cfg(feature = "reqif")]
    fn import_reqif(&self, path: &Path) -> Result<Vec<Requirement>> {
        use pyo3::prelude::*;
        use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods};

        let path_str = path.to_string_lossy().to_string();

        Python::with_gil(|py| -> Result<Vec<Requirement>> {
            let reqif_module = py
                .import("reqif")
                .map_err(|e| Error::Parse(format!("Failed to import reqif: {}", e)))?;

            let parser_class = reqif_module
                .getattr("ReqIFZParser")
                .map_err(|e| Error::Parse(format!("Failed to get ReqIFZParser: {}", e)))?;

            let parser = parser_class
                .call1((&path_str,))
                .map_err(|e| Error::Parse(format!("Failed to create parser: {}", e)))?;

            let reqif_obj = parser
                .call_method0("parse")
                .map_err(|e| Error::Parse(format!("Failed to parse ReqIF: {}", e)))?;

            let spec_objects_attr = reqif_obj
                .getattr("spec_objects")
                .map_err(|e| Error::Parse(format!("Failed to get spec_objects: {}", e)))?;

            let spec_objects = spec_objects_attr
                .downcast::<PyDict>()
                .map_err(|e| Error::Parse(format!("spec_objects is not a dict: {}", e)))?;

            let mut requirements = Vec::new();

            for (_, spec_obj) in spec_objects.iter() {
                let obj_dict = spec_obj
                    .downcast::<PyDict>()
                    .map_err(|e| Error::Parse(format!("spec_obj is not a dict: {}", e)))?;

                let attributes_attr = obj_dict
                    .getattr("attribute_values")
                    .map_err(|e| Error::Parse(format!("Failed to get attribute_values: {}", e)))?;

                let attributes = attributes_attr
                    .downcast::<PyDict>()
                    .map_err(|e| {
                        Error::Parse(format!("attribute_values is not a dict: {}", e))
                    })?;

                let id = self
                    .extract_attribute(&attributes, &self.mapping.id_attribute)?
                    .unwrap_or_else(|| format!("REQ-{}", requirements.len() + 1));

                let text = self
                    .extract_attribute(&attributes, &self.mapping.text_attribute)?
                    .unwrap_or_default();

                let title = if let Some(title_attr) = &self.mapping.title_attribute {
                    self.extract_attribute(&attributes, title_attr)?
                        .unwrap_or_else(|| id.clone())
                } else {
                    id.clone()
                };

                let status = if let Some(status_attr) = &self.mapping.status_attribute {
                    let status_str = self
                        .extract_attribute(&attributes, status_attr)?
                        .unwrap_or_else(|| "draft".to_string());
                    self.map_status(&status_str)
                } else {
                    RequirementStatus::Draft
                };

                let spec_type: String = obj_dict
                    .getattr("spec_type")
                    .map_err(|e| Error::Parse(format!("Failed to get spec_type: {}", e)))?
                    .extract()
                    .unwrap_or_else(|_| "LLR".to_string());
                let req_type = self.map_type(&spec_type);

                let parent = if let Some(parent_attr) = &self.mapping.parent_attribute {
                    self.extract_attribute(&attributes, parent_attr)?
                } else {
                    None
                };

                let uuid: Option<String> = obj_dict
                    .getattr("identifier")
                    .ok()
                    .and_then(|v: pyo3::Bound<'_, pyo3::PyAny>| v.extract::<String>().ok());

                let mut req = Requirement::new(id, req_type, title);
                req.text = text;
                req.status = status;
                req.parent = parent;
                if let Some(uuid_str) = uuid {
                    req.aliases.push(uuid_str);
                }

                requirements.push(req);
            }

            Ok(requirements)
        })
    }

    #[cfg(feature = "reqif")]
    fn export_reqif(&self, requirements: &[Requirement], path: &Path) -> Result<()> {
        use pyo3::prelude::*;

        let path_str = path.to_string_lossy().to_string();

        Python::with_gil(|py| -> Result<()> {
            let reqif_module = py
                .import("reqif")
                .map_err(|e| Error::Parse(format!("Failed to import reqif: {}", e)))?;

            let builder_class = reqif_module
                .getattr("ReqIFBuilder")
                .map_err(|e| Error::Parse(format!("Failed to get ReqIFBuilder: {}", e)))?;

            let builder = builder_class
                .call0()
                .map_err(|e| Error::Parse(format!("Failed to create builder: {}", e)))?;

            for req in requirements {
                let spec_obj = builder
                    .call_method1("create_spec_object", (req.id.clone(),))
                    .map_err(|e| Error::Parse(format!("Failed to create spec_object: {}", e)))?;

                spec_obj
                    .call_method1("set_attribute", (&self.mapping.id_attribute, &req.id))
                    .map_err(|e| Error::Parse(format!("Failed to set id attribute: {}", e)))?;

                spec_obj
                    .call_method1("set_attribute", (&self.mapping.text_attribute, &req.text))
                    .map_err(|e| Error::Parse(format!("Failed to set text attribute: {}", e)))?;

                if let Some(title_attr) = &self.mapping.title_attribute {
                    spec_obj
                        .call_method1("set_attribute", (title_attr, &req.title))
                        .map_err(|e| {
                            Error::Parse(format!("Failed to set title attribute: {}", e))
                        })?;
                }

                if let Some(status_attr) = &self.mapping.status_attribute {
                    spec_obj
                        .call_method1("set_attribute", (status_attr, req.status.as_str()))
                        .map_err(|e| {
                            Error::Parse(format!("Failed to set status attribute: {}", e))
                        })?;
                }

                builder
                    .call_method1("add_spec_object", (spec_obj,))
                    .map_err(|e| Error::Parse(format!("Failed to add spec_object: {}", e)))?;
            }

            let reqif_obj = builder
                .call_method0("build")
                .map_err(|e| Error::Parse(format!("Failed to build ReqIF: {}", e)))?;

            reqif_obj
                .call_method1("write", (&path_str,))
                .map_err(|e| Error::Parse(format!("Failed to write ReqIF: {}", e)))?;

            Ok(())
        })
    }

    #[cfg(feature = "reqif")]
    fn extract_attribute(
        &self,
        attributes: &pyo3::Bound<'_, pyo3::types::PyDict>,
        name: &str,
    ) -> Result<Option<String>> {
        use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods};

        if let Ok(Some(value)) = attributes.get_item(name) {
            if let Ok(s) = value.extract::<String>() {
                return Ok(Some(s));
            }
            if let Ok(dict) = value.downcast::<PyDict>() {
                if let Ok(Some(v)) = dict.get_item("value") {
                    if let Ok(s) = v.extract::<String>() {
                        return Ok(Some(s));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Map a ReqIF status string to an internal `RequirementStatus`.
    pub fn map_status(&self, status: &str) -> RequirementStatus {
        let lower = status.to_lowercase();
        if let Some(mapped) = self.mapping.status_mapping.get(&lower) {
            RequirementStatus::from_str(mapped).unwrap_or(RequirementStatus::Draft)
        } else {
            RequirementStatus::from_str(status).unwrap_or(RequirementStatus::Draft)
        }
    }

    /// Map a ReqIF spec-type string to an internal `RequirementType`.
    pub fn map_type(&self, spec_type: &str) -> RequirementType {
        let upper = spec_type.to_uppercase();
        if let Some(t) = &self.mapping.hlr_type {
            if upper.contains(&t.to_uppercase()) {
                return RequirementType::Hlr;
            }
        }
        if let Some(t) = &self.mapping.llr_type {
            if upper.contains(&t.to_uppercase()) {
                return RequirementType::Llr;
            }
        }
        if let Some(t) = &self.mapping.tst_type {
            if upper.contains(&t.to_uppercase()) {
                return RequirementType::Tst;
            }
        }
        if upper.contains("HLR") || upper.contains("HIGH") {
            RequirementType::Hlr
        } else if upper.contains("TST") || upper.contains("TEST") {
            RequirementType::Tst
        } else {
            RequirementType::Llr
        }
    }
}

impl Default for ReqIfAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RequirementAdapter for ReqIfAdapter {
    fn name(&self) -> &'static str {
        "reqif"
    }

    fn read(&self, source: &Path) -> Result<Vec<Requirement>> {
        #[cfg(feature = "reqif")]
        {
            if source
                .extension()
                .map(|e| e == "reqif" || e == "reqifz")
                .unwrap_or(false)
            {
                return self.import_reqif(source);
            }
            Err(Error::Parse(
                "Source must be a .reqif or .reqifz file".to_string(),
            ))
        }
        #[cfg(not(feature = "reqif"))]
        {
            let _ = source;
            Err(Error::Config(
                "ReqIF support not enabled. Recompile with --features reqif".to_string(),
            ))
        }
    }

    fn write(&self, requirements: &[Requirement], target: &Path) -> Result<()> {
        #[cfg(feature = "reqif")]
        {
            self.export_reqif(requirements, target)
        }
        #[cfg(not(feature = "reqif"))]
        {
            let _ = (requirements, target);
            Err(Error::Config(
                "ReqIF support not enabled. Recompile with --features reqif".to_string(),
            ))
        }
    }

    fn can_handle(&self, path: &Path) -> bool {
        path.extension()
            .map(|e| e == "reqif" || e == "reqifz")
            .unwrap_or(false)
    }
}
