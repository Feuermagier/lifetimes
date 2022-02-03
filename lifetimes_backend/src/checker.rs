use std::{cell::RefCell, fmt::Debug, fmt::Display};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarId(usize);

pub struct Vars {
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

    pub fn create_var(&mut self, is_mut: bool, identifier: String) -> VarId {
        self.add_var(Var {
            status: VarStatus::Unitialized,
            valid: true,
            is_mut,
            identifier,
            id: VarId(0),
            deref_var: None,
            parent: None,
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
                id: VarId(0),
                deref_var: None,
                parent: Some(derefed_var),
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
    id: VarId,
    deref_var: Option<VarId>,
    parent: Option<VarId>,
}

impl Var {
    fn assert_usable(&self) {
        if !self.valid {
            panic!("Local '{}' is not valid anymore", &self.identifier);
        }

        if self.status == VarStatus::Unitialized {
            panic!("Local '{}' has not yet been initialized", &self.identifier);
        }

        if self.status == VarStatus::Moved {
            panic!("Local '{}' has been moved", &self.identifier);
        }
    }

    pub fn transition_initialized(&mut self, vars: &Vars) {
        match &self.status {
            VarStatus::Borrowed(borrowers) => borrowers
                .iter()
                .for_each(|borrower| Self::invalidate_var(*borrower, vars)),
            VarStatus::MutBorrowed(borrower) => Self::invalidate_var(*borrower, vars),
            VarStatus::Moved => {
                if !self.is_mut {
                    panic!(
                        "Local '{}' is not mutable, so only one assignment is allowed",
                        &self.identifier
                    )
                }
            }
            VarStatus::Unitialized | VarStatus::Initialized => {}
        }
        self.status = VarStatus::Initialized;
        self.valid = true;
    }

    pub fn transition_mut_borrowed(&mut self, borrower: VarId, vars: &Vars) {
        if !self.is_mut {
            panic!(
                "Local '{}' is not mutable, so you cannot borrow it mutable",
                &self.identifier
            );
        }

        self.assert_usable();

        match &self.status {
            VarStatus::Initialized => {}
            VarStatus::Borrowed(borrowers) => borrowers
                .iter()
                .for_each(|borrower| Self::invalidate_var(*borrower, vars)),
            VarStatus::MutBorrowed(borrower) => Self::invalidate_var(*borrower, vars),
            VarStatus::Unitialized | VarStatus::Moved => unreachable!(), // Already covered by self.assert_usable()
        }
        self.status = VarStatus::MutBorrowed(borrower);
    }

    pub fn transition_borrowed(&mut self, borrower: VarId, vars: &Vars) {
        self.assert_usable();

        match &mut self.status {
            VarStatus::Initialized => self.status = VarStatus::Borrowed(vec![borrower]),
            VarStatus::Borrowed(borrowers) => borrowers.push(borrower),
            VarStatus::MutBorrowed(prev_borrower) => {
                Self::invalidate_var(*prev_borrower, vars);
                self.status = VarStatus::Borrowed(vec![borrower]);
            }
            VarStatus::Unitialized | VarStatus::Moved => unreachable!(), // Already covered by self.assert_usable()
        }
    }

    pub fn transition_moved(&mut self, vars: &Vars) {
        self.assert_usable();

        match &self.status {
            VarStatus::Initialized => {}
            VarStatus::Borrowed(borrowers) => borrowers
                .iter()
                .for_each(|borrower| Self::invalidate_var(*borrower, vars)),
            VarStatus::MutBorrowed(borrower) => Self::invalidate_var(*borrower, vars),
            VarStatus::Unitialized | VarStatus::Moved => unreachable!(), // Already covered by self.assert_usable()
        }

        self.status = VarStatus::Moved;
    }

    fn invalidate_var(var: VarId, vars: &Vars) {
        vars.resolve_var(var).borrow_mut().invalidate(vars);
    }

    fn invalidate(&mut self, vars: &Vars) {
        self.valid = false;

        match &self.status {
            VarStatus::Unitialized | VarStatus::Initialized | VarStatus::Moved => {}
            VarStatus::Borrowed(borrowers) => borrowers
                .iter()
                .for_each(|borrower| Self::invalidate_var(*borrower, vars)),
            VarStatus::MutBorrowed(borrower) => Self::invalidate_var(*borrower, vars),
        }

        if let Some(parent) = self.parent {
            Self::invalidate_var(parent, vars);
        }
    }
}

impl Display for Var {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.valid {
            f.write_str(&format!(
                "Local '{}' ({}) (invalid)",
                self.identifier, self.id.0
            ))
        } else {
            f.write_str(&format!(
                "Local '{}' ({}) {:?}",
                self.identifier, self.id.0, self.status
            ))
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
