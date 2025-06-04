//! **Warning**: `unsafe` code
//!
//! A simple wrapper
//!
//! Note that this mod won't be a complete rusty wrapper since I only need
//! a few functions from libclang

#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
use clang_sys::*;

use anyhow::{Result, anyhow};
use std::ffi::*;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub struct Cursor(CXCursor);

unsafe fn cxs_to_string(cxs: CXString) -> Option<String> {
    if cxs.data.is_null() {
        return None;
    }
    let res = unsafe { CStr::from_ptr(clang_getCString(cxs)) }
        .to_string_lossy()
        .to_string();
    unsafe { clang_disposeString(cxs) };
    Some(res)
}

impl Cursor {
    pub fn get_kind(&self) -> Option<CursorKind> {
        unsafe {
            match clang_getCursorKind(self.0) {
                _ => None,
            }
        }
    }

    pub fn visit_children<U>(
        &self,
        callback: fn(cursor: Cursor, parent: Cursor, userdata: &mut U) -> ChildVisitResult,
        userdata: &mut U,
    ) {
        struct ClientData<'a, U> {
            callback: fn(cursor: Cursor, parent: Cursor, userdata: &mut U) -> ChildVisitResult,
            userdata: &'a mut U,
        }

        extern "C" fn visit_callback<U>(
            cursor: CXCursor,
            parent: CXCursor,
            client_data: CXClientData,
        ) -> CXChildVisitResult {
            let client_data = client_data as *mut _ as *mut ClientData<U>;
            unsafe {
                ((*client_data).callback)(Cursor(cursor), Cursor(parent), (*client_data).userdata)
                    as i32
            }
        }

        let mut client_data = ClientData { callback, userdata };

        unsafe {
            clang_visitChildren(
                self.0,
                visit_callback::<U>,
                (&mut client_data) as *mut _ as *mut c_void,
            );
        }
    }

    pub fn get_spelling(&self) -> Option<String> {
        unsafe { cxs_to_string(clang_getCursorSpelling(self.0)) }
    }

    pub fn get_raw_comment(&self) -> Option<String> {
        unsafe { cxs_to_string(clang_Cursor_getRawCommentText(self.0)) }
    }

    pub fn get_parsed_comment(&self) -> ParsedComment {
        unsafe {
            let comment = clang_Cursor_getParsedComment(self.0);
            ParsedComment(comment)
        }
    }
}

#[derive(Debug)]
pub struct Index(CXIndex);

impl Index {
    pub fn new(exclude: i32, display: i32) -> Result<Self> {
        unsafe {
            let index = clang_createIndex(exclude, display);
            if index.is_null() {
                Err(anyhow!("Failed to create index"))
            } else {
                Ok(Self(index))
            }
        }
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        unsafe {
            clang_disposeIndex(self.0);
        }
    }
}

#[derive(Debug)]
pub struct TranslationUnit(CXTranslationUnit);

impl TranslationUnit {
    pub fn new(filepath: &Path) -> Result<Self> {
        let arguments = vec![c"-xc++".as_ptr(), c"-std=c++20".as_ptr()];

        Self::with_arguments(filepath, &arguments)
    }

    pub fn with_arguments(filepath: &Path, arguments: &[*const c_char]) -> Result<Self> {
        unsafe {
            if !filepath.is_file() {
                return Err(anyhow!("{:?} is not a file", filepath));
            }

            let index = Index::new(0, 1)?;
            let file = CString::new(filepath.to_str().unwrap()).unwrap();
            let raw_tu = clang_parseTranslationUnit(
                index.0,
                file.as_ptr(),
                arguments.as_ptr(),
                arguments.len() as c_int,
                std::ptr::null_mut(),
                0,
                CXTranslationUnit_None,
            );

            const SYNTAX_ERROR: &'static str = "failed to parse source file due to error";
            if raw_tu.is_null() {
                return Err(anyhow!(SYNTAX_ERROR));
            }

            let diagnostic_num = clang_getNumDiagnostics(raw_tu);
            for diag_id in 0..diagnostic_num {
                let diag = clang_getDiagnostic(raw_tu, diag_id);
                if clang_getDiagnosticSeverity(diag) == CXDiagnostic_Error {
                    return Err(anyhow!(SYNTAX_ERROR));
                }
            }

            Ok(Self(raw_tu))
        }
    }
}

impl Drop for TranslationUnit {
    fn drop(&mut self) {
        unsafe { clang_disposeTranslationUnit(self.0) }
    }
}

pub enum CursorKind {
    Namespace,
    ClassDecl,
    StructDecl,
    FunctionDecl,
    CXXMethod,
    ClassTemplate,
    FunctionTemplate,
}

pub struct ParsedComment(CXComment);

impl ParsedComment {
    fn get_kind(&self) -> CommentKind {
        unsafe { clang_Comment_getKind(self.0).into() }
    }

    pub fn get_text(&self) -> Option<String> {
        match self.get_kind() {
            CommentKind::Null => return None,
            CommentKind::FullComment => unsafe {
                cxs_to_string(clang_FullComment_getAsHTML(self.0))
            },
            _ => unimplemented!(),
        }
    }
}

pub enum CommentKind {
    Null,
    Text,
    InlineCommand,
    HTMLStartTag,
    HTMLEndTag,
    Paragraph,
    BlockCommand,
    ParamCommand,
    TParamCommand,
    VerbatimBlockCommand,
    VerbatimBlockLine,
    VervatimLine,
    FullComment,
}

impl From<CXCommentKind> for CommentKind {
    fn from(value: CXCommentKind) -> Self {
        match value {
            CXComment_Null => CommentKind::Null,
            CXComment_Text => CommentKind::Text,
            CXComment_InlineCommand => CommentKind::InlineCommand,
            CXComment_HTMLStartTag => CommentKind::HTMLStartTag,
            CXComment_HTMLEndTag => CommentKind::HTMLEndTag,
            CXComment_Paragraph => CommentKind::Paragraph,
            CXComment_BlockCommand => CommentKind::BlockCommand,
            CXComment_ParamCommand => CommentKind::ParamCommand,
            CXComment_TParamCommand => CommentKind::TParamCommand,
            CXComment_VerbatimBlockCommand => CommentKind::VerbatimBlockCommand,
            CXComment_VerbatimBlockLine => CommentKind::VerbatimBlockLine,
            CXComment_VerbatimLine => CommentKind::VervatimLine,
            CXComment_FullComment => CommentKind::FullComment,
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(i32)]
pub enum ChildVisitResult {
    Continue = CXChildVisit_Continue,
    Break = CXChildVisit_Break,
    Recurse = CXChildVisit_Recurse,
}

struct File<'a>(CXFile, PhantomData<&'a TranslationUnit>);

impl<'a> File<'a> {
    pub fn new(tu: &'a TranslationUnit, filepath: &Path) -> Result<Self> {
        unsafe {
            let c_pathfile = CString::new(filepath.to_str().unwrap()).unwrap();
            let raw_file = clang_getFile(tu.0, c_pathfile.as_ptr());

            if raw_file.is_null() {
                return Err(anyhow!("failed to get target file"));
            }

            Ok(Self(raw_file, PhantomData))
        }
    }
}
