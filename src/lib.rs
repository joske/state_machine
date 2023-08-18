use anyhow::Result;
use std::fmt::Formatter;
use std::hash::Hash;
use std::sync::RwLock;
use std::{collections::HashMap, fmt::Display};
use tracing::{debug, error};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct State<F>
where
    F: Fn() -> Result<()> + Clone,
{
    name: String,
    events: HashMap<Event, Transition<F>>,
}

impl<F> State<F>
where
    F: Fn() -> Result<()> + Clone,
{
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
    /// * `action` - an optional action to execute when the event is triggered. Make sure this
    /// action never panics.
    pub fn add_event(&mut self, event: Event, new_state: State<F>, action: Option<F>)
    where
        F: Fn() -> Result<()> + Clone,
    {
        let t = Transition {
            trigger: event.clone(),
            new_state,
            action,
        };
        self.events.insert(event, t);
    }
}

impl<F> Display for State<F>
where
    F: Fn() -> Result<()> + Clone,
{
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
    new_state: State<F>,
    action: Option<F>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct StateMachine<F>
where
    F: Fn() -> Result<()> + Clone,
{
    name: String,
    state: RwLock<State<F>>,
    initial_state: State<F>,
}

impl<F> StateMachine<F>
where
    F: Fn() -> Result<()> + Clone,
{
    #[must_use]
    /// Create a new state machine
    /// # Arguments
    /// * `name` - the name of this state machine
    /// * `initial_state` - the initial state of the machine
    pub fn new(name: impl Into<String>, initial_state: &State<F>) -> Self {
        Self {
            name: name.into(),
            state: RwLock::new(initial_state.clone()),
            initial_state: initial_state.clone(),
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
        let transition = state.events.get(event).cloned();
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
    pub fn current_state(&self) -> State<F> {
        self.state.read().expect("failed to get lock").clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tracing_test::traced_test;

    #[test]
    fn test_state_clone() {
        let mut initial: State<fn() -> Result<()>> = State::new("initial");
        let e1 = Event::new("e1");
        initial.add_event(e1.clone(), initial.clone(), None);
        assert_eq!(1, initial.events.len());
        let clone = initial.clone();
        assert_eq!(1, clone.events.len());
    }

    #[test]
    fn test_state_with_action_clone() {
        let mut initial: State<fn() -> Result<()>> = State::new("initial");
        let e1 = Event::new("e1");
        initial.add_event(e1.clone(), initial.clone(), Some(|| Ok(())));
        assert_eq!(1, initial.events.len());
        let clone = initial.clone();
        assert_eq!(1, clone.events.len());
    }

    #[traced_test]
    #[test]
    fn test_one_state() -> Result<()> {
        let mut initial: State<fn() -> Result<()>> = State::new("initial");
        let e1 = Event::new("e1");
        initial.add_event(e1.clone(), initial.clone(), None);
        let machine = StateMachine::new("test", &initial);

        machine.event(&e1)?;
        assert_eq!(machine.current_state().name, "initial");
        Ok(())
    }

    #[traced_test]
    #[test]
    fn test_two_states() -> Result<()> {
        let mut initial = State::new("initial");
        let e1 = Event::new("e1");
        let second = State::new("second");
        let action_called = AtomicBool::new(false);
        let action = || {
            debug!("action directe!");
            action_called.store(true, Ordering::SeqCst);
            Ok(())
        };
        initial.add_event(e1.clone(), second.clone(), Some(action));
        let machine = StateMachine::new("test", &initial);

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
    fn test_action_fails() -> Result<()> {
        let mut initial = State::new("initial");
        let e1 = Event::new("e1");
        let second = State::new("second");
        let action_called = AtomicBool::new(false);
        let action = || {
            debug!("action directe!");
            action_called.store(true, Ordering::SeqCst);
            Err(anyhow::anyhow!("action failed"))
        };
        initial.add_event(e1.clone(), second.clone(), Some(action));
        let machine = StateMachine::new("test", &initial);

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
        let mut initial = State::new("initial");
        let e1 = Event::new("e1");
        let second = State::new("second");
        initial.add_event(e1.clone(), second.clone(), Some(regular_function));
        let machine = StateMachine::new("test", &initial);

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
        let mut initial = State::new("initial");
        let e1 = Event::new("e1");
        let second = State::new("second");
        let action = || {
            panic!("action failed");
        };
        initial.add_event(e1.clone(), second.clone(), Some(action));
        let machine = StateMachine::new("test", &initial);

        machine.event(&e1).unwrap();
    }
}
