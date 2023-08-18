use anyhow::Result;
use std::fmt::Formatter;
use std::hash::Hash;
use std::sync::RwLock;
use std::{collections::HashMap, fmt::Display};
use tracing::{debug, error};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

impl Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}'", self.name)
    }
}

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
struct Transition<F>
where
    F: Fn() -> Result<()> + Clone,
{
    trigger: Event,
    new_state: State,
    action: Option<F>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct StateMachine<F>
where
    F: Fn() -> Result<()> + Clone,
{
    name: String,
    state: RwLock<State>,
    initial_state: State,
    events: HashMap<State, HashMap<Event, Transition<F>>>,
}

impl<F> StateMachine<F>
where
    F: Fn() -> Result<()> + Clone,
{
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
        let state_events = self.events.get(&state);
        if let Some(state_events) = state_events {
            let transition = state_events.get(event).cloned();
            if let Some(transition) = transition {
                let new_state = transition.new_state.clone();
                debug!(
                    "{}: {:?} -> {:?}",
                    self.name,
                    state.name,
                    new_state.clone().name,
                );
                *state = new_state;
                if let Some(action) = transition.action {
                    return action();
                }
                Ok(())
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

pub struct StateMachineBuilder<F>
where
    F: Fn() -> Result<()> + Clone,
{
    name: String,
    state: Option<RwLock<State>>,
    initial_state: Option<State>,
    events: HashMap<State, HashMap<Event, Transition<F>>>,
}

impl<F> StateMachineBuilder<F>
where
    F: Fn() -> Result<()> + Clone,
{
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            state: None,
            initial_state: None,
            events: HashMap::new(),
        }
    }

    #[must_use]
    pub fn initial_state(mut self, initial_state: &State) -> Self {
        self.initial_state = Some(initial_state.clone());
        self.state = Some(RwLock::new(initial_state.clone()));
        self
    }

    #[must_use]
    pub fn add_event(
        mut self,
        old_state: State,
        event: Event,
        new_state: State,
        action: Option<F>,
    ) -> Self
    where
        F: Fn() -> Result<()> + Clone,
    {
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
    pub fn build(self) -> StateMachine<F>
    where
        F: Fn() -> Result<()> + Clone,
    {
        let name = self.name;
        let state = self.state.expect("initial state not set");
        let initial_state = self.initial_state.expect("initial state not set");
        StateMachine {
            name,
            state,
            initial_state,
            events: self.events,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn test_one_state() -> Result<()> {
        let initial = State::new("initial");
        let e1 = Event::new("e1");
        let machine: StateMachine<fn() -> Result<()>> = StateMachineBuilder::new("test")
            .initial_state(&initial)
            .add_event(initial.clone(), e1.clone(), initial.clone(), None)
            .build();

        machine.event(&e1)?;
        assert_eq!(machine.current_state().name, "initial");
        Ok(())
    }

    #[traced_test]
    #[test]
    fn test_two_states() -> Result<()> {
        let initial = State::new("initial");
        let second = State::new("second");
        let e1 = Event::new("e1");
        let action_called = AtomicBool::new(false);
        let action = || {
            debug!("action directe!");
            action_called.store(true, Ordering::SeqCst);
            Ok(())
        };
        let machine = StateMachineBuilder::new("test")
            .initial_state(&initial)
            .add_event(initial.clone(), e1.clone(), second.clone(), Some(action))
            .build();

        machine.event(&e1)?;
        assert_eq!(machine.current_state().name, "second");
        // in `second` state, there are no transitions
        assert!(machine.event(&e1).is_err());
        assert!(action_called.load(Ordering::SeqCst));
        machine.reset();

        // check if we can call the action again
        assert_eq!(machine.current_state().name, "initial");
        action_called.store(false, Ordering::SeqCst);
        assert!(!action_called.load(Ordering::SeqCst));
        machine.event(&e1)?;
        assert_eq!(machine.current_state().name, "second");
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
        let machine: StateMachine<fn() -> Result<()>> = StateMachineBuilder::new("test")
            .initial_state(&initial)
            .add_event(initial.clone(), e1.clone(), second.clone(), None)
            .add_event(second.clone(), e2.clone(), initial.clone(), None)
            .build();

        assert_eq!(machine.current_state(), initial);
        machine.event(&e1)?;
        assert_eq!(machine.current_state().name, "second");
        machine.event(&e2)?;
        assert_eq!(machine.current_state().name, "initial");
        Ok(())
    }

    #[traced_test]
    #[test]
    fn test_action_fails() -> Result<()> {
        let initial = State::new("initial");
        let second = State::new("second");
        let e1 = Event::new("e1");
        let action_called = AtomicBool::new(false);
        let action = || {
            debug!("action directe!");
            action_called.store(true, Ordering::SeqCst);
            Err(anyhow::anyhow!("action failed"))
        };
        let machine = StateMachineBuilder::new("test")
            .initial_state(&initial)
            .add_event(initial.clone(), e1.clone(), second.clone(), Some(action))
            .build();

        let result = machine.event(&e1);
        assert_eq!(machine.current_state().name, "second");
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
        let machine = StateMachineBuilder::new("test")
            .initial_state(&initial)
            .add_event(
                initial.clone(),
                e1.clone(),
                second.clone(),
                Some(regular_function),
            )
            .build();

        machine.event(&e1)?;
        assert_eq!(machine.current_state().name, "second");
        // in seconde state, there are no transitions
        assert!(machine.event(&e1).is_err());
        machine.reset();
        assert_eq!(machine.current_state().name, "initial");
        Ok(())
    }

    #[traced_test]
    #[test]
    #[should_panic]
    fn test_panics() -> () {
        let initial = State::new("initial");
        let e1 = Event::new("e1");
        let action = || {
            panic!("action failed");
        };
        let machine = StateMachineBuilder::new("test")
            .initial_state(&initial)
            .add_event(initial.clone(), e1.clone(), initial.clone(), Some(action))
            .build();

        machine.event(&e1).unwrap();
    }
}
