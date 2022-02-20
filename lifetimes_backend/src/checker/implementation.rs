use std::{cell::RefCell, fmt::Display};

use log::{debug, trace};

use super::{CheckerError, CheckerResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarId(usize);

impl Display for VarId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("${}", self.0))
    }
}

#[derive(Debug)]
pub struct Vars {
    void_literal: VarId,
    vars: Vec<RefCell<Var>>,
}

impl Display for Vars {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for var in &self.vars {
            f.write_str(&format!("{}\n", var.borrow()))?;
        }

        Ok(())
    }
}

impl Vars {
    pub fn new(static_origin: OriginId) -> Self {
        Self {
            vars: vec![RefCell::new(Var {
                status: VarStatus::Initialized,
                valid: true,
                identifier: "()".to_string(),
                is_mut: false,
                is_copy: true,
                id: VarId(0),
                deref_var: None,
                parent: None,
                borrows: Vec::new(),
                origin: static_origin,
            })],
            void_literal: VarId(0),
        }
    }

    pub fn void_literal(&self) -> VarId {
        self.void_literal
    }

    pub fn create_var(
        &mut self,
        origin: OriginId,
        is_mut: bool,
        is_copy: bool,
        identifier: String,
    ) -> VarId {
        self.add_var(Var {
            status: VarStatus::Unitialized,
            valid: true,
            is_mut,
            is_copy,
            identifier,
            id: VarId(0),
            deref_var: None,
            parent: None,
            borrows: Vec::new(),
            origin,
        })
    }

    /// For refs (i.e. assumes the value is copy)
    pub fn create_tmp(&mut self, origin: OriginId, is_mut: bool, text: String) -> VarId {
        self.add_var(Var {
            status: VarStatus::Unitialized,
            valid: true,
            identifier: "<tmp> ".to_string() + &text,
            is_mut,
            is_copy: true,
            id: VarId(0),
            deref_var: None,
            parent: None,
            borrows: Vec::new(),
            origin,
        })
    }

    pub fn create_literal(&mut self, origin: OriginId, literal: String) -> VarId {
        self.add_var(Var {
            status: VarStatus::Initialized,
            valid: true,
            identifier: "<lit> ".to_string() + &literal,
            is_mut: false,
            is_copy: true,
            id: VarId(0),
            deref_var: None,
            parent: None,
            borrows: Vec::new(),
            origin,
        })
    }

    pub fn get_deref_var(&mut self, derefed_var: VarId) -> VarId {
        let deref_var = self.resolve_var(derefed_var).borrow().deref_var;
        if let Some(deref_var) = deref_var {
            deref_var
        } else {
            let is_mut = self.resolve_var(derefed_var).borrow().is_mut;
            let identifier = "*".to_string() + &self.resolve_var(derefed_var).borrow().identifier;
            let origin = self.resolve_var(derefed_var).borrow().origin;
            let deref_var = self.add_var(Var {
                status: VarStatus::Unitialized,
                valid: true,
                identifier,
                is_mut,
                is_copy: false, // TODO TODO TODO
                id: VarId(0),
                deref_var: None,
                parent: Some(derefed_var),
                borrows: Vec::new(),
                origin,
            });
            self.resolve_var(derefed_var).borrow_mut().deref_var = Some(deref_var);
            deref_var
        }
    }

    pub fn resolve_var(&self, id: VarId) -> &RefCell<Var> {
        &self.vars[id.0]
    }

    fn add_var(&mut self, mut var: Var) -> VarId {
        let id = VarId(self.vars.len());
        var.id = id;
        debug!("Created var '{}' {}", &var.identifier, id);
        self.vars.push(RefCell::new(var));
        id
    }
}

#[derive(Debug)]
pub struct Var {
    status: VarStatus,
    valid: bool,
    identifier: String,
    is_mut: bool,
    is_copy: bool,
    id: VarId,
    deref_var: Option<VarId>,
    parent: Option<VarId>,
    borrows: Vec<Borrow>,
    origin: OriginId,
}

impl Var {
    pub fn assert_usable(&self) -> CheckerResult {
        if !self.valid {
            Err(CheckerError::Invalid(self.identifier.to_string(), self.id))
        } else if self.status == VarStatus::Unitialized {
            Err(CheckerError::Uninitialized(
                self.identifier.to_string(),
                self.id,
            ))
        } else if self.status == VarStatus::Moved {
            Err(CheckerError::Moved(self.identifier.to_string(), self.id))
        } else {
            Ok(())
        }
    }

    fn assert_assignable(&self) -> CheckerResult {
        if self.status != VarStatus::Unitialized && !self.is_mut {
            Err(CheckerError::ImmutableAssigned(
                self.identifier.clone(),
                self.id,
            ))
        } else {
            Ok(())
        }
    }

    fn invalidate_borrowers(&self, vars: &Vars) {
        match &self.status {
            VarStatus::Borrowed(borrowers) => borrowers
                .iter()
                .for_each(|borrower| self.invalidate_var(*borrower, vars)),
            VarStatus::MutBorrowed(borrower) => self.invalidate_var(*borrower, vars),
            _ => {}
        }
    }

    pub fn validate_for_origin(
        &self,
        origin: OriginId,
        origins: &Origins,
        vars: &Vars,
    ) -> CheckerResult {
        // TODO
        Ok(())
    }

    /*
        1. Make sure self can be assigned to (i.e. it is either uninitialized or mutable)
        2. Because this value will be dropped, all borrowers are invalidated
        3. The status of self is updated
        4. The value_sources are transitioned into a moved state (and invalidated by doing so if they are not copy)
        5. If the value_sources borrowed vars, self's borrowed vars are updated to reflect that self now also borrows them
    */
    /// value_sources must contain at least one value
    pub fn initialize_with_values(
        &mut self,
        value_sources: Vec<VarId>,
        vars: &Vars,
    ) -> CheckerResult {
        debug!("Initializing {} from {:?}", self.id, value_sources);

        self.assert_assignable()?;

        self.invalidate_borrowers(vars);

        self.status = VarStatus::Initialized;
        self.valid = true;

        self.borrows = Vec::new();

        for value_source in value_sources {
            self.borrows.extend(
                vars.resolve_var(value_source)
                    .borrow_mut()
                    .transition_moved(vars)?,
            );
        }

        // Notify all variables that are now borrowed by us of their new borrower
        for borrow in &self.borrows {
            match borrow {
                Borrow::Mutable(borrow) => {
                    vars.resolve_var(*borrow)
                        .borrow_mut()
                        .transition_mut_borrowed(self.id, vars)?;
                }
                Borrow::Immutable(borrow) => {
                    vars.resolve_var(*borrow)
                        .borrow_mut()
                        .transition_borrowed(self.id, vars)?;
                }
            }
        }

        Ok(())
    }

    /*
        1. Make sure self can be assigned to (i.e. it is either uninitialized or mutable)
        2. Because this value will be dropped, all borrowers are invalidated
        3. The status of self is updated
        4. borrowed_vars are updated to reflect that self now borrows them
    */
    pub fn initialize_with_borrows(
        &mut self,
        is_mut: bool,
        borrowed_vars: Vec<VarId>,
        vars: &Vars,
    ) -> CheckerResult {
        debug!("Initializing {} as a borrow from {:?}", self.id, borrowed_vars);

        self.assert_assignable()?;

        self.invalidate_borrowers(vars);

        self.status = VarStatus::Initialized;
        self.valid = true;

        self.borrows = Vec::with_capacity(borrowed_vars.len());
        for borrowed_var in borrowed_vars {
            if is_mut {
                self.borrows.push(Borrow::Mutable(borrowed_var));
                vars.resolve_var(borrowed_var)
                    .borrow_mut()
                    .transition_mut_borrowed(self.id, vars)?;
            } else {
                self.borrows.push(Borrow::Immutable(borrowed_var));
                vars.resolve_var(borrowed_var)
                    .borrow_mut()
                    .transition_borrowed(self.id, vars)?;
            }
        }

        Ok(())
    }

    fn transition_mut_borrowed(&mut self, borrower: VarId, vars: &Vars) -> CheckerResult {
        trace!("{} got borrowed mutably by {}", self.id, borrower);

        if !self.is_mut {
            return Err(CheckerError::ImmutableBorrowedMutable(
                self.identifier.clone(),
                self.id,
            ));
        }

        self.assert_usable()?;

        match &self.status {
            VarStatus::Initialized => {}
            VarStatus::Borrowed(borrowers) => borrowers
                .iter()
                .for_each(|borrower| self.invalidate_var(*borrower, vars)),
            VarStatus::MutBorrowed(borrower) => self.invalidate_var(*borrower, vars),
            VarStatus::Unitialized | VarStatus::Moved => unreachable!(), // Already covered by self.assert_usable()
        }
        self.status = VarStatus::MutBorrowed(borrower);

        Ok(())
    }

    fn transition_borrowed(&mut self, borrower: VarId, vars: &Vars) -> CheckerResult {
        trace!("{} got borrowed by {}", self.id, borrower);

        self.assert_usable()?;

        if let VarStatus::Initialized = &self.status {
            self.status = VarStatus::Borrowed(vec![borrower]);
        } else if let VarStatus::Borrowed(borrowers) = &mut self.status {
            borrowers.push(borrower)
        } else if let VarStatus::MutBorrowed(prev_borrower) = self.status {
            self.invalidate_var(prev_borrower, vars);
            self.status = VarStatus::Borrowed(vec![borrower]);
        } else {
            unreachable!(); // Already covered by self.assert_usable()
        }

        Ok(())
    }

    // Returns the status of self to replicate it in the var that received the move
    fn transition_moved(&mut self, vars: &Vars) -> Result<Vec<Borrow>, CheckerError> {
        trace!("{} got moved", self.id);

        self.assert_usable()?;

        if self.is_copy {
            return Ok(self.borrows.clone());
        }

        match &self.status {
            VarStatus::Initialized => {}
            VarStatus::Borrowed(borrowers) => borrowers
                .iter()
                .for_each(|borrower| self.invalidate_var(*borrower, vars)),
            VarStatus::MutBorrowed(borrower) => self.invalidate_var(*borrower, vars),
            VarStatus::Unitialized | VarStatus::Moved => unreachable!(), // Already covered by self.assert_usable()
        }

        self.status = VarStatus::Moved;

        Ok(std::mem::replace(&mut self.borrows, Vec::new()))
    }

    fn invalidate_var(&self, var: VarId, vars: &Vars) {
        vars.resolve_var(var).borrow_mut().invalidate(self.id, vars);
    }

    fn invalidate(&mut self, invalidated_by: VarId, vars: &Vars) {
        let mut invalidator_found = false;
        for borrow in &self.borrows {
            match borrow {
                Borrow::Mutable(borrow) | Borrow::Immutable(borrow) => {
                    if *borrow == invalidated_by {
                        invalidator_found = true;
                    }
                }
            }
        }
        if !invalidator_found {
            return;
        }
        trace!("{} got invalidated by {}", self.id, invalidated_by);

        self.valid = false;

        match &self.status {
            VarStatus::Unitialized | VarStatus::Initialized | VarStatus::Moved => {}
            VarStatus::Borrowed(borrowers) => borrowers
                .iter()
                .for_each(|borrower| self.invalidate_var(*borrower, vars)),
            VarStatus::MutBorrowed(borrower) => self.invalidate_var(*borrower, vars),
        }

        if let Some(parent) = self.parent {
            self.invalidate_var(parent, vars); // TODO
        }
    }
}

impl Display for Var {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.valid {
            f.write_str(&format!(
                "Local '{}' ${} (invalid)",
                self.identifier, self.id.0
            ))
        } else {
            f.write_str(&format!(
                "Local '{}' ${} {:?}",
                self.identifier, self.id.0, self.status
            ))?;

            f.write_str(&format!(" (borrows {:?})", self.borrows))?;

            Ok(())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum VarStatus {
    Borrowed(Vec<VarId>),
    MutBorrowed(VarId),
    Unitialized,
    Initialized,
    Moved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Borrow {
    Mutable(VarId),
    Immutable(VarId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginId(usize);

pub struct Origins {
    parent_origins: Vec<Option<OriginId>>,
}

impl Origins {
    pub fn new() -> Self {
        Self {
            parent_origins: Vec::new(),
        }
    }

    pub fn create_unbound_origin(&mut self) -> OriginId {
        let id = self.parent_origins.len();
        self.parent_origins.push(None);
        OriginId(id)
    }

    pub fn create_bound_origin(&mut self, parent: OriginId) -> OriginId {
        let id = self.parent_origins.len();
        self.parent_origins.push(Some(parent));
        OriginId(id)
    }

    pub fn resolve_parent(&self, origin: OriginId) -> Option<OriginId> {
        self.parent_origins[origin.0]
    }
}

impl OriginId {
    fn has_parent(self, parent: OriginId, origins: &Origins) -> bool {
        let mut origin = self;
        while let Some(p) = origins.resolve_parent(origin) {
            if p == parent {
                return true;
            }
            origin = p;
        }
        false
    }
}

impl Display for OriginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}
