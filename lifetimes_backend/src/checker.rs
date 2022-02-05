use std::{cell::RefCell, fmt::Debug, fmt::Display};

use log::{debug, trace};

pub(crate) type CheckerResult = Result<(), CheckerError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarId(usize);

impl Display for VarId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("${}", self.0))
    }
}

#[derive(Debug)]
pub(crate) struct Vars {
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
    pub fn new() -> Self {
        Self { vars: Vec::new() }
    }

    pub(crate) fn create_var(&mut self, is_mut: bool, is_copy: bool, identifier: String) -> VarId {
        self.add_var(Var {
            status: VarStatus::Unitialized,
            valid: true,
            is_mut,
            is_copy,
            identifier,
            id: VarId(0),
            deref_var: None,
            parent: None,
            borrows: Borrows::None,
        })
    }

    /// For refs (i.e. assumes the value is copy)
    pub(crate) fn create_tmp(&mut self, is_mut: bool, text: String) -> VarId {
        self.add_var(Var {
            status: VarStatus::Unitialized,
            valid: true,
            identifier: "<tmp> ".to_string() + &text,
            is_mut,
            is_copy: true,
            id: VarId(0),
            deref_var: None,
            parent: None,
            borrows: Borrows::None,
        })
    }

    pub fn create_literal(&mut self, literal: String) -> VarId {
        self.add_var(Var {
            status: VarStatus::Initialized,
            valid: true,
            identifier: "<lit> ".to_string() + &literal,
            is_mut: false,
            is_copy: true,
            id: VarId(0),
            deref_var: None,
            parent: None,
            borrows: Borrows::None,
        })
    }

    pub fn get_deref_var(&mut self, derefed_var: VarId) -> VarId {
        let deref_var = self.resolve_var(derefed_var).borrow().deref_var;
        if let Some(deref_var) = deref_var {
            deref_var
        } else {
            let is_mut = self.resolve_var(derefed_var).borrow().is_mut;
            let identifier = "*".to_string() + &self.resolve_var(derefed_var).borrow().identifier;
            let deref_var = self.add_var(Var {
                status: VarStatus::Unitialized,
                valid: true,
                identifier,
                is_mut,
                is_copy: false, // TODO TODO TODO
                id: VarId(0),
                deref_var: None,
                parent: Some(derefed_var),
                borrows: Borrows::None,
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
pub(crate) struct Var {
    status: VarStatus,
    valid: bool,
    identifier: String,
    is_mut: bool,
    is_copy: bool,
    id: VarId,
    deref_var: Option<VarId>,
    parent: Option<VarId>,
    borrows: Borrows,
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

    /*
        1. Make sure self can be assigned to (i.e. it is either uninitialized or mutable)
        2. Because this value will be dropped, all borrowers are invalidated
        3. The status of self is updated
        4. The value_source is transitioned into a moved state (and invalidated by doing so if is not copy)
        5. If value_source borrowed vars, self's the borrowed vars are updated to reflect that self now also borrows them
    */
    pub fn initialize_with_value(&mut self, value_source: VarId, vars: &Vars) -> CheckerResult {
        debug!("Initializing {} from {}", self.id, value_source);

        self.assert_assignable()?;

        self.invalidate_borrowers(vars);

        self.status = VarStatus::Initialized;
        self.valid = true;

        self.borrows = vars.resolve_var(value_source)
            .borrow_mut()
            .transition_moved(vars)?; // This must be done before copying the source_status, because value_source will be invalidated while doing so and the move will fail
        drop(value_source); // Because value_source is now moved, it must not been used later in this fn

        match &self.borrows {
            Borrows::Mutable(borrows) => {
                for borrow in borrows {
                    vars.resolve_var(*borrow)
                    .borrow_mut()
                    .transition_mut_borrowed(self.id, vars)?;
                }
            }
            Borrows::Immutable(borrows) => {
                for borrow in borrows {
                    vars.resolve_var(*borrow)
                    .borrow_mut()
                    .transition_borrowed(self.id, vars)?;
                }
            }
            Borrows::None => self.borrows = Borrows::None,
        }

        Ok(())
    }

    /*
        1. Make sure self can be assigned to (i.e. it is either uninitialized or mutable)
        2. Because this value will be dropped, all borrowers are invalidated
        3. The status of self is updated
        4. borrowed_var is updated to reflect that this now borrows it
    */
    pub fn initialize_with_borrow(
        &mut self,
        is_mut: bool,
        borrowed_var: VarId,
        vars: &Vars,
    ) -> CheckerResult {
        debug!("Initializing {} as a borrow from {}", self.id, borrowed_var);

        self.assert_assignable()?;

        self.invalidate_borrowers(vars);

        self.status = VarStatus::Initialized;
        self.valid = true;

        if is_mut {
            self.borrows = Borrows::Mutable(vec![borrowed_var]);
            vars.resolve_var(borrowed_var)
                .borrow_mut()
                .transition_mut_borrowed(self.id, vars)?;
        } else {
            self.borrows = Borrows::Immutable(vec![borrowed_var]);
            vars.resolve_var(borrowed_var)
                .borrow_mut()
                .transition_borrowed(self.id, vars)?;
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
    fn transition_moved(&mut self, vars: &Vars) -> Result<Borrows, CheckerError> {
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
        self.borrows = Borrows::None;

        Ok(std::mem::replace(&mut self.borrows, Borrows::None))
    }

    fn invalidate_var(&self, var: VarId, vars: &Vars) {
        vars.resolve_var(var).borrow_mut().invalidate(self.id, vars);
    }

    fn invalidate(&mut self, invalidated_by: VarId, vars: &Vars) {
        match &self.borrows {
            Borrows::Mutable(borrows) | Borrows::Immutable(borrows) => {
                if !borrows.contains(&invalidated_by) {
                    return;
                }
            }
            Borrows::None => return,
        };
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

            match &self.borrows {
                Borrows::Mutable(borrows) | Borrows::Immutable(borrows) => {
                    f.write_str(&format!(" (borrows {:?})", borrows))?
                }
                Borrows::None => {}
            }

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
enum Borrows {
    Mutable(Vec<VarId>),
    Immutable(Vec<VarId>),
    None,
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
}
