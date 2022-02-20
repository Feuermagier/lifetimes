mod implementation;

use std::fmt::{Debug, Display};

use log::debug;

pub use self::implementation::VarId;
use self::implementation::{OriginId, Origins, Vars};

pub type CheckerResult = Result<(), CheckerError>;

pub struct Checker {
    vars: Vars,
    origins: Origins,
    scope: Option<OriginId>,
    static_origin: OriginId,
}

impl Checker {
    pub fn new() -> Self {
        let mut origins = Origins::new();
        let static_origin = origins.create_unbound_origin();
        Self {
            vars: Vars::new(static_origin),
            origins,
            scope: None,
            static_origin,
        }
    }

    pub fn enter_function(&mut self, return_origin: OriginId) {
        self.scope = Some(return_origin)
    }

    /// The initial scope is already entered for you, so only call this when entering an inner scope
    pub fn enter_scope(&mut self) {
        self.scope = Some(if let Some(parent_scope) = self.scope {
            self.origins.create_bound_origin(parent_scope)
        } else {
            self.origins.create_unbound_origin()
        });
        debug!("Entered scope {}", self.scope.unwrap());
    }

    /// Don't leave the outermost scope
    pub fn leave_scope(&mut self, return_var: Option<VarId>) -> CheckerResult {
        let parent_scope = self
            .origins
            .resolve_parent(self.scope.unwrap())
            .ok_or(CheckerError::OutermostScopeLeft)?;

        if let Some(return_var) = return_var {
            self.vars
                .resolve_var(return_var)
                .borrow()
                .validate_for_origin(parent_scope, &self.origins, &self.vars)?;
        }

        self.scope = Some(parent_scope);
        debug!("Left scope {}", self.scope.unwrap());
        Ok(())
    }

    pub fn create_var(&mut self, is_mut: bool, is_copy: bool, identifier: String) -> VarId {
        self.vars
            .create_var(self.scope.unwrap(), is_mut, is_copy, identifier)
    }

    pub fn create_ref_tmp(&mut self, is_mut: bool, text: String) -> VarId {
        self.vars.create_tmp(self.scope.unwrap(), is_mut, text)
    }

    pub fn create_literal(&mut self, literal: String) -> VarId {
        self.vars.create_literal(self.scope.unwrap(), literal)
    }

    pub fn void_literal(&self) -> VarId {
        self.vars.void_literal()
    }

    pub fn initialize_var_with_value(&self, var: VarId, value_sources: Vec<VarId>) -> CheckerResult {
        self.vars
            .resolve_var(var)
            .borrow_mut()
            .initialize_with_values(value_sources, &self.vars)
    }

    pub fn initialize_var_with_borrow(
        &self,
        var: VarId,
        borrowed_vars: Vec<VarId>,
        is_mut: bool,
    ) -> CheckerResult {
        self.vars
            .resolve_var(var)
            .borrow_mut()
            .initialize_with_borrows(is_mut, borrowed_vars, &self.vars)
    }

    pub fn get_deref_var(&mut self, var: VarId) -> VarId {
        self.vars.get_deref_var(var)
    }

    pub fn check_var_usable(&self, var: VarId) -> CheckerResult {
        self.vars.resolve_var(var).borrow().assert_usable()
    }

    pub fn static_origin(&self) -> OriginId {
        self.static_origin
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
