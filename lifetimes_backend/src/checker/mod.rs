mod implementation;

use std::fmt::{Debug, Display};

use log::debug;

pub use self::implementation::VarId;
use self::implementation::{OriginId, Origins, Vars};

pub type CheckerResult = Result<(), CheckerError>;

pub struct Checker {
    vars: Vars,
    origins: Origins,
    scope: OriginId,
}

impl Checker {
    pub fn new() -> Self {
        let mut origins = Origins::new();
        let scope = origins.create_unbound_origin();
        Self {
            vars: Vars::new(),
            origins,
            scope,
        }
    }

    /// The initial scope is already entered for you, so only call this when entering an inner scope
    pub fn enter_scope(&mut self) {
        self.scope = self.origins.create_bound_origin(self.scope);
        debug!("Entered scope {}", self.scope);
    }

    /// Don't leave the outermost scope
    pub fn leave_scope(&mut self, return_var: VarId) -> CheckerResult {
        let parent_scope = self
            .origins
            .resolve_parent(self.scope)
            .ok_or(CheckerError::OutermostScopeLeft)?;

        self.vars
            .resolve_var(return_var)
            .borrow()
            .validate_for_origin(parent_scope, &self.origins, &self.vars)?;

        self.scope = parent_scope;
        debug!("Left scope {}", self.scope);
        Ok(())
    }

    pub fn create_var(&mut self, is_mut: bool, is_copy: bool, identifier: String) -> VarId {
        self.vars
            .create_var(self.scope, is_mut, is_copy, identifier)
    }

    pub fn create_ref_tmp(&mut self, is_mut: bool, text: String) -> VarId {
        self.vars.create_tmp(self.scope, is_mut, text)
    }

    pub fn create_literal(&mut self, literal: String) -> VarId {
        self.vars.create_literal(self.scope, literal)
    }

    pub fn initialize_var_with_value(&self, var: VarId, value_source: VarId) -> CheckerResult {
        self.vars
            .resolve_var(var)
            .borrow_mut()
            .initialize_with_value(value_source, &self.vars)
    }

    pub fn initialize_var_with_borrow(
        &self,
        var: VarId,
        borrowed_var: VarId,
        is_mut: bool,
    ) -> CheckerResult {
        self.vars
            .resolve_var(var)
            .borrow_mut()
            .initialize_with_borrow(is_mut, borrowed_var, &self.vars)
    }

    pub fn get_deref_var(&mut self, var: VarId) -> VarId {
        self.vars.get_deref_var(var)
    }

    pub fn check_var_usable(&self, var: VarId) -> CheckerResult {
        self.vars.resolve_var(var).borrow().assert_usable()
    }
}

impl Display for Checker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.vars)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CheckerError {
    #[error("Local '{0}' {1} is not valid anymore")]
    Invalid(String, VarId),

    #[error("Local '{0}' {1} has not yet been initialized")]
    Uninitialized(String, VarId),

    #[error("Local '{0}' {1} has been moved")]
    Moved(String, VarId),

    #[error("Local '{0}' {1} is immutable, so exactly one assignment is allowed")]
    ImmutableAssigned(String, VarId),

    #[error("Local '{0}' {1} is immutable, so it cannot be borrowed mutably")]
    ImmutableBorrowedMutable(String, VarId),

    #[error("Local '{0}' {1} is not valid for origin {2}")]
    InvalidOrigin(String, VarId, OriginId),

    #[error("Outermost scope left")]
    OutermostScopeLeft,
}
