//! Emergency relief: the console commands that shed weather and the day cycle, and those that
//! put back exactly what was there (a world that had the cycle off keeps it off).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Changed {
    weather: Option<Weather>,
    time: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Weather {
    Rain,
    Thunder,
}

#[derive(Debug, Default)]
pub struct Relief {
    pub(crate) changed: Option<Changed>,
}

impl Relief {
    pub fn active(&self) -> bool {
        self.changed.is_some()
    }

    /// Relief starts with the world as it is now; the commands that shed load.
    pub fn start(&mut self, clear_weather: bool, freeze_time: bool, raining: bool, thundering: bool, advance_time: bool) -> Vec<String> {
        if self.changed.is_some() {
            return Vec::new();
        }
        let weather = match (thundering, raining) {
            _ if !clear_weather => None,
            (true, _) => Some(Weather::Thunder),
            (false, true) => Some(Weather::Rain),
            (false, false) => None,
        };
        let time = freeze_time && advance_time;
        self.changed = Some(Changed { weather, time });
        let mut out = Vec::new();
        if weather.is_some() {
            out.push("weather clear".into());
        }
        if time {
            out.push("gamerule advance_time false".into());
        }
        out
    }

    /// Relief ends (or the server stops): the commands that put the world back.
    pub fn end(&mut self) -> Vec<String> {
        let Some(c) = self.changed.take() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        match c.weather {
            Some(Weather::Thunder) => out.push("weather thunder".into()),
            Some(Weather::Rain) => out.push("weather rain".into()),
            None => {}
        }
        if c.time {
            out.push("gamerule advance_time true".into());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thunderstorm_and_the_cycle_are_shed_and_put_back() {
        let mut r = Relief::default();
        assert_eq!(r.start(true, true, true, true, true), ["weather clear", "gamerule advance_time false"]);
        assert!(r.start(true, true, false, false, false).is_empty());
        assert_eq!(r.end(), ["weather thunder", "gamerule advance_time true"]);
        assert!(r.end().is_empty());
    }

    #[test]
    fn what_was_off_stays_off() {
        let mut r = Relief::default();
        // Clear sky and the cycle already off: nothing to shed, nothing to restore.
        assert!(r.start(true, true, false, false, false).is_empty());
        assert!(r.active());
        assert!(r.end().is_empty());
        // Rain, with weather clearing switched off in the config.
        assert_eq!(r.start(false, true, true, false, true), ["gamerule advance_time false"]);
        assert_eq!(r.end(), ["gamerule advance_time true"]);
        assert_eq!(r.start(true, false, true, false, true), ["weather clear"]);
        assert_eq!(r.end(), ["weather rain"]);
    }
}
