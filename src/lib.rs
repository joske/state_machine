use anyhow::Result;
use derive_more::Display;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use tracing::{debug, error};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display)]
pub struct State {
    name: String,
}

impl State {
    /// Create a new state
    /// # Arguments
    /// * `name` - the name of the state
    /// # Returns
    /// The new state
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, Display)]
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

#[allow(dead_code)]
struct Transition {
    trigger: Event,
    new_state: State,
    action: Option<Box<dyn Fn() -> Result<()>>>,
}

#[allow(dead_code)]
pub struct StateMachine {
    name: String,
    state: RwLock<State>,
    initial_state: State,
    events: HashMap<State, HashMap<Event, Transition>>,
}

impl StateMachine {
    /// Handle an event
    /// # Errors
    /// If no transition is found for the event in the current state
    /// or if the action fails
    /// or if the lock is poisoned
    pub fn event(&self, event: &Event) -> Result<()> {
        debug!("handling event: {event}");
        let mut state = self
            .state
            .write()
            .map_err(|_| anyhow::anyhow!("lock error"))?;
        let state_events = self.events.get(&state);
        if let Some(state_events) = state_events {
            let transition = state_events.get(event);
            if let Some(transition) = transition {
                let new_state = transition.new_state.clone();
                debug!("{}: {} -> {}", self.name, state, new_state.clone());
                *state = new_state;
                if let Some(ref action) = transition.action {
                    action()
                } else {
                    // no action, just return Ok
                    Ok(())
                }
            } else {
                error!("no transition found for event {event} in state {state}");
                Err(anyhow::anyhow!(
                    "no transition found for event {event} in state {state}"
                ))
            }
        } else {
            error!("no transition found for event {event} in state {state}");
            Err(anyhow::anyhow!(
                "no transition found for event {event} in state {state}"
            ))
        }
    }

    /// Reset the state machine to its initial state
    /// #Panics
    /// If the lock is poisoned
    pub fn reset(&self) {
        let mut state = self.state.write().expect("failed to get lock");
        *state = self.initial_state.clone();
    }

    /// Get the current state
    /// #Panics
    /// If the lock is poisoned
    pub fn current_state(&self) -> State {
        self.state.read().expect("failed to get lock").clone()
    }
}

pub struct StateMachineBuilder {
    name: String,
    state: RwLock<State>,
    initial_state: State,
    events: HashMap<State, HashMap<Event, Transition>>,
}

impl StateMachineBuilder {
    #[must_use]
    pub fn new(name: impl Into<String>, initial_state: &State) -> Self {
        Self {
            name: name.into(),
            state: RwLock::new(initial_state.clone()),
            initial_state: initial_state.clone(),
            events: HashMap::new(),
        }
    }

    #[must_use]
    /// Add an event to the state machine
    /// # Arguments
    /// * `old_state` - the state in which the event is handled
    ///            (the state before the transition)
    /// * `event` - the event
    /// * `new_state` - the state after the transition
    /// * `action` - an optional action to execute when the event is handled
    /// Make sure this never panics - as this would poison the lock and cause the state machine to fail
    pub fn add_event(
        mut self,
        old_state: State,
        event: Event,
        new_state: State,
        action: Option<Box<dyn Fn() -> Result<()>>>,
    ) -> Self {
        let state_events = self.events.entry(old_state).or_insert_with(HashMap::new);
        let t = Transition {
            trigger: event.clone(),
            new_state,
            action,
        };
        state_events.insert(event, t);
        self
    }

    #[must_use]
    pub fn build(self) -> StateMachine {
        StateMachine {
            name: self.name,
            state: self.state,
            initial_state: self.initial_state,
            events: self.events,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn test_one_state() -> Result<()> {
        let initial = State::new("initial");
        let e1 = Event::new("e1");
        let machine = StateMachineBuilder::new("test", &initial)
            .add_event(initial.clone(), e1.clone(), initial.clone(), None)
            .build();

        machine.event(&e1)?;
        assert_eq!(machine.current_state(), initial);
        Ok(())
    }

    #[traced_test]
    #[test]
    fn test_two_states() -> Result<()> {
        let initial = State::new("initial");
        let second = State::new("second");
        let e1 = Event::new("e1");
        let action_called = Arc::new(AtomicBool::new(false));
        let action_called_clone = action_called.clone();
        let action = Box::new(move || {
            debug!("action directe!");
            action_called_clone.store(true, Ordering::SeqCst);
            Ok(())
        });
        let machine = StateMachineBuilder::new("test", &initial)
            .add_event(initial.clone(), e1.clone(), second.clone(), Some(action))
            .build();

        machine.event(&e1)?;
        assert_eq!(machine.current_state(), second);
        // in `second` state, there are no transitions
        assert!(machine.event(&e1).is_err());
        assert!(action_called.load(Ordering::SeqCst));
        machine.reset();

        // check if we can call the action again
        assert_eq!(machine.current_state(), initial);
        action_called.store(false, Ordering::SeqCst);
        assert!(!action_called.load(Ordering::SeqCst));
        machine.event(&e1)?;
        assert_eq!(machine.current_state(), second);
        assert!(action_called.load(Ordering::SeqCst));
        Ok(())
    }

    #[traced_test]
    #[test]
    fn test_two_states_circular() -> Result<()> {
        let initial = State::new("initial");
        let second = State::new("second");
        let e1 = Event::new("e1");
        let e2 = Event::new("e2");
        let action_called = Arc::new(AtomicBool::new(false));
        let action_called_clone = action_called.clone();
        let action1 = Box::new(move || {
            debug!("turn on");
            action_called_clone.store(true, Ordering::SeqCst);
            Ok(())
        });
        let action_called_clone2 = action_called.clone();
        let action2 = Box::new(move || {
            debug!("turn off");
            action_called_clone2.store(false, Ordering::SeqCst);
            Ok(())
        });
        let machine = StateMachineBuilder::new("test", &initial)
            .add_event(initial.clone(), e1.clone(), second.clone(), Some(action1))
            .add_event(second.clone(), e2.clone(), initial.clone(), Some(action2))
            .build();

        assert_eq!(machine.current_state(), initial);
        assert!(!action_called.load(Ordering::SeqCst));
        machine.event(&e1)?;
        assert_eq!(machine.current_state(), second);
        assert!(action_called.load(Ordering::SeqCst));
        machine.event(&e2)?;
        assert_eq!(machine.current_state(), initial);
        assert!(!action_called.load(Ordering::SeqCst));
        Ok(())
    }

    #[traced_test]
    #[test]
    fn test_action_fails() -> Result<()> {
        let initial = State::new("initial");
        let second = State::new("second");
        let e1 = Event::new("e1");
        let action_called = Arc::new(AtomicBool::new(false));
        let action_called_clone = action_called.clone();
        let action = Box::new(move || {
            debug!("action directe!");
            action_called_clone.store(true, Ordering::SeqCst);
            Err(anyhow::anyhow!("action failed"))
        });
        let machine = StateMachineBuilder::new("test", &initial)
            .add_event(initial.clone(), e1.clone(), second.clone(), Some(action))
            .build();

        let result = machine.event(&e1);
        assert_eq!(machine.current_state(), second);
        assert!(result.is_err());
        Ok(())
    }

    fn regular_function() -> Result<()> {
        debug!("action indirecte!");
        Ok(())
    }

    #[traced_test]
    #[test]
    fn test_regular_function() -> Result<()> {
        let initial = State::new("initial");
        let second = State::new("second");
        let e1 = Event::new("e1");
        let machine = StateMachineBuilder::new("test", &initial)
            .add_event(
                initial.clone(),
                e1.clone(),
                second.clone(),
                Some(Box::new(regular_function)),
            )
            .build();

        machine.event(&e1)?;
        assert_eq!(machine.current_state(), second);
        // in seconde state, there are no transitions
        assert!(machine.event(&e1).is_err());
        machine.reset();
        assert_eq!(machine.current_state(), initial);
        Ok(())
    }

    #[traced_test]
    #[test]
    #[should_panic]
    fn test_panics() -> () {
        let initial = State::new("initial");
        let e1 = Event::new("e1");
        let action = Box::new(|| {
            panic!("action failed");
        });
        let machine = StateMachineBuilder::new("test", &initial)
            .add_event(initial.clone(), e1.clone(), initial.clone(), Some(action))
            .build();

        machine.event(&e1).unwrap();
    }
}
