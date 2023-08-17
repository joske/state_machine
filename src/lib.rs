use anyhow::Result;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use tracing::debug;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct State {
    name: String,
    events: HashMap<Event, Transition>,
}

impl State {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events: HashMap::new(),
        }
    }

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

type Action = fn() -> Result<()>;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct Event {
    name: String,
}

impl Event {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
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
    pub fn new(initial_state: State) -> Self {
        Self {
            state: RwLock::new(initial_state.clone()),
            initial_state: initial_state.clone(),
            states: vec![initial_state],
        }
    }

    pub fn add_state(&mut self, state: State) {
        self.states.push(state);
    }

    pub fn event(&self, event: Event) -> Result<()> {
        debug!("handling event: {:?}", event);
        let mut state = self
            .state
            .write()
            .map_err(|_| anyhow::anyhow!("lock error"))?;
        debug!("state: {:?}", state);
        let transition = state.events.get(&event).cloned();
        if let Some(transition) = transition {
            *state = transition.new_state.clone();
            debug!("new state: {:?}", self.state);
            if let Some(action) = transition.action {
                return action();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_states() -> Result<()> {
        tracing_subscriber::fmt::init();
        let mut initial = State::new("initial");
        let e1 = Event::new("e1");
        let second = State::new("second");
        initial.add_event(
            e1.clone(),
            second.clone(),
            Some(|| {
                println!("action");
                Ok(())
            }),
        );
        let mut machine = StateMachine::new(initial.clone());
        machine.add_state(second);
        machine.event(e1)?;
        assert_eq!(machine.state.read().unwrap().name, "second");
        Ok(())
    }
}
