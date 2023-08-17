use anyhow::Result;
use std::fmt::Formatter;
use std::hash::Hash;
use std::sync::RwLock;
use std::{collections::HashMap, fmt::Display};
use tracing::{debug, error};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct State {
    name: String,
    events: HashMap<Event, Transition>,
}

impl State {
    /// Create a new state
    /// # Arguments
    /// * `name` - the name of the state
    /// # Returns
    /// The new state
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events: HashMap::new(),
        }
    }

    /// Add a transition to the state
    /// # Arguments
    /// * `event` - the trigger for the transition
    /// * `new_state` - the new state after the event
    /// * `action` - an optional action to execute when the event is triggered
    pub fn add_event(&mut self, event: Event, new_state: State, action: Option<Action>) {
        let t = Transition {
            old_state: self.clone(),
            trigger: event.clone(),
            new_state,
            action,
        };
        self.events.insert(event, t);
    }
}

impl Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self.name)
    }
}

type Action = fn() -> Result<()>;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct Event {
    name: String,
}

impl Event {
    /// Create a new event
    /// # Arguments
    /// * `name` - the name of the event
    /// # Returns
    /// The new event
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self.name)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Transition {
    old_state: State,
    trigger: Event,
    new_state: State,
    action: Option<Action>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct StateMachine {
    state: RwLock<State>,
    initial_state: State,
    states: Vec<State>,
}

impl StateMachine {
    #[must_use]
    /// Create a new state machine
    /// # Arguments
    /// * `initial_state` - the initial state of the machine
    /// * `states` - the list of all states
    pub fn new(initial_state: &State, states: Vec<State>) -> Self {
        Self {
            state: RwLock::new(initial_state.clone()),
            initial_state: initial_state.clone(),
            states,
        }
    }

    /// Handle an event
    /// # Errors
    /// If no transition is found for the event in the current state
    /// or if the action fails
    /// or if the lock is poisoned
    pub fn event(&self, event: &Event) -> Result<()> {
        debug!("handling event: {:?}", event);
        let mut state = self
            .state
            .write()
            .map_err(|_| anyhow::anyhow!("lock error"))?;
        debug!("state: {:?}", state);
        let transition = state.events.get(event).cloned();
        if let Some(transition) = transition {
            *state = transition.new_state.clone();
            debug!("new state: {:?}", self.state);
            if let Some(action) = transition.action {
                return action();
            }
            Ok(())
        } else {
            error!("no transition found for event {event} in state {state}");
            Err(anyhow::anyhow!(
                "no transition found for event {event} in state {state}"
            ))?
        }
    }

    /// Reset the state machine to its initial state
    pub fn reset(&self) {
        let mut state = self.state.write().expect("failed to get lock");
        *state = self.initial_state.clone();
    }

    /// Get the current state
    pub fn current_state(&self) -> State {
        self.state.read().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    use super::*;

    #[traced_test]
    #[test]
    fn test_two_states() -> Result<()> {
        let mut initial = State::new("initial");
        let e1 = Event::new("e1");
        let second = State::new("second");
        let action = || {
            debug!("action directe!");
            Ok(())
        };
        initial.add_event(e1.clone(), second.clone(), Some(action));
        let states = vec![initial.clone(), second.clone()];
        let machine = StateMachine::new(&initial, states);

        machine.event(&e1)?;
        assert_eq!(machine.current_state().name, "second");
        // in seconde state, there are no transitions
        assert!(machine.event(&e1).is_err());
        machine.reset();
        assert_eq!(machine.current_state().name, "initial");
        Ok(())
    }
}
