use std::borrow::Borrow;
use std::cell::RefCell;
use std::ops::DerefMut;
use std::rc::Rc;
use yew::{Component, ComponentLink, Html, Properties};

thread_local! {
    static CURRENT_HOOK: RefCell<Option<HookState>> = RefCell::new(None);
}

struct HookState {
    counter: usize,
    process_message: Rc<dyn Fn(Box<dyn FnOnce() -> bool>)>,
    hooks: Vec<Rc<RefCell<dyn std::any::Any>>>,
}

pub trait FunctionProvider {
    type TProps: Properties + PartialEq;
    fn run(props: &Self::TProps) -> Html;
}

pub struct FunctionComponent<T: FunctionProvider> {
    _never: std::marker::PhantomData<T>,
    props: T::TProps,
    hook_state: RefCell<Option<HookState>>,
}

impl<T: 'static> Component for FunctionComponent<T>
where
    T: FunctionProvider,
{
    type Message = Box<dyn FnOnce() -> bool>;
    type Properties = T::TProps;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        FunctionComponent {
            _never: std::marker::PhantomData::default(),
            props,
            hook_state: RefCell::new(Some(HookState {
                counter: 0,
                process_message: Rc::new(move |msg| link.send_message(msg)),
                hooks: vec![],
            })),
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        msg()
    }

    fn change(&mut self, props: Self::Properties) -> bool {
        let mut props = props;
        std::mem::swap(&mut self.props, &mut props);
        props == self.props
    }

    //noinspection DuplicatedCode
    fn view(&self) -> Html {
        // Reset hook
        self.hook_state
            .try_borrow_mut()
            .expect("Unexpected concurrent/nested view call")
            .as_mut()
            .unwrap()
            .counter = 0;
        // Load hook
        CURRENT_HOOK.with(|previous_hook| {
            std::mem::swap(
                previous_hook
                    .try_borrow_mut()
                    .expect("Previous hook still borrowed")
                    .deref_mut(),
                self.hook_state.borrow_mut().deref_mut(),
            );
        });

        let ret = T::run(&self.props);

        // Unload hook
        CURRENT_HOOK.with(|previous_hook| {
            std::mem::swap(
                previous_hook
                    .try_borrow_mut()
                    .expect("Previous hook still borrowed")
                    .deref_mut(),
                self.hook_state.borrow_mut().deref_mut(),
            );
        });

        return ret;
    }
}

pub fn use_ref<T: 'static, InitialProvider>(initial_value: InitialProvider) -> Rc<RefCell<T>>
where
    InitialProvider: FnOnce() -> T,
{
    type UseRefState<T> = Rc<RefCell<T>>;

    use_hook(
        |state: &mut UseRefState<T>, pretrigger_change_acceptor| {
            let _ignored = || pretrigger_change_acceptor(|_| false); // we need it to be a specific closure type, even if we never use it
            return state.clone();
        },
        move || Rc::new(RefCell::new(initial_value())),
    )
}

pub fn use_reducer<Action: 'static, Reducer, State: 'static>(
    reducer: Reducer,
    initial_state: State,
) -> (Rc<State>, Box<impl Fn(Action)>)
where
    Reducer: Fn(Rc<State>, Action) -> State + 'static,
{
    return use_reducer_with_init(reducer, initial_state, |a| a);
}

pub fn use_reducer_with_init<Action: 'static, Reducer, State: 'static, InitialState, InitFn>(
    reducer: Reducer,
    initial_state: InitialState,
    init: InitFn,
) -> (Rc<State>, Box<impl Fn(Action)>)
where
    Reducer: Fn(Rc<State>, Action) -> State + 'static,
    InitFn: Fn(InitialState) -> State,
{
    struct UseReducerState<State> {
        current_state: Rc<State>,
    }
    let init = Box::new(init);
    let reducer = Rc::new(reducer);
    let ret = use_hook(
        |internal_hook_change: &mut UseReducerState<State>, pretrigger_change_runner| {
            return (
                internal_hook_change.current_state.clone(),
                Box::new(move |action: Action| {
                    let reducer = reducer.clone();
                    pretrigger_change_runner(
                        move |internal_hook_change: &mut UseReducerState<State>| {
                            internal_hook_change.current_state = Rc::new((reducer)(
                                internal_hook_change.current_state.clone(),
                                action,
                            ));
                            true
                        },
                    );
                }),
            );
        },
        move || UseReducerState {
            current_state: Rc::new(init(initial_state)),
        },
    );
    return ret;
}

pub fn use_state<T, F>(initial_state_fn: F) -> (Rc<T>, Box<impl Fn(T)>)
where
    F: FnOnce() -> T,
    T: 'static,
{
    struct UseStateState<T2> {
        current: Rc<T2>,
    }
    return use_hook(
        |prev: &mut UseStateState<T>, hook_update| {
            let current = prev.current.clone();
            return (
                current,
                Box::new(move |o: T| {
                    hook_update(|state: &mut UseStateState<T>| {
                        state.current = Rc::new(o);
                        true
                    });
                }),
            );
        },
        move || UseStateState {
            current: Rc::new(initial_state_fn()),
        },
    );
}

pub fn use_effect<F, Destructor>(callback: F)
where
    F: FnOnce() -> Destructor,
    Destructor: FnOnce() + 'static,
{
    let callback = Box::new(callback);
    use_effect5(
        Box::new(|_: &(), _: &(), _: &(), _: &(), _: &()| callback()),
        (),
        (),
        (),
        (),
        (),
    );
}

pub fn use_effect1<F, Destructor, T1>(callback: F, o1: T1)
where
    F: FnOnce(&T1) -> Destructor,
    Destructor: FnOnce() + 'static,
    T1: PartialEq + 'static,
{
    let callback = Box::new(callback);
    use_effect5(
        Box::new(|a: &T1, _: &(), _: &(), _: &(), _: &()| callback(a)),
        o1,
        (),
        (),
        (),
        (),
    );
}

pub fn use_effect2<F, Destructor, T1, T2>(callback: F, o1: T1, o2: T2)
where
    F: FnOnce(&T1, &T2) -> Destructor,
    Destructor: FnOnce() + 'static,
    T1: PartialEq + 'static,
    T2: PartialEq + 'static,
{
    let callback = Box::new(callback);
    use_effect5(
        Box::new(|a: &T1, b: &T2, _: &(), _: &(), _: &()| callback(a, b)),
        o1,
        o2,
        (),
        (),
        (),
    );
}

pub fn use_effect3<F, Destructor, T1, T2, T3>(callback: F, o1: T1, o2: T2, o3: T3)
where
    F: FnOnce(&T1, &T2, &T3) -> Destructor,
    Destructor: FnOnce() + 'static,
    T1: PartialEq + 'static,
    T2: PartialEq + 'static,
    T3: PartialEq + 'static,
{
    let callback = Box::new(callback);
    use_effect5(
        Box::new(|a: &T1, b: &T2, c: &T3, _: &(), _: &()| callback(a, b, c)),
        o1,
        o2,
        o3,
        (),
        (),
    );
}

pub fn use_effect4<F, Destructor, T1, T2, T3, T4>(callback: F, o1: T1, o2: T2, o3: T3, o4: T4)
where
    F: FnOnce(&T1, &T2, &T3, &T4) -> Destructor,
    Destructor: FnOnce() + 'static,
    T1: PartialEq + 'static,
    T2: PartialEq + 'static,
    T3: PartialEq + 'static,
    T4: PartialEq + 'static,
{
    let callback = Box::new(callback);
    use_effect5(
        Box::new(|a: &T1, b: &T2, c: &T3, d: &T4, _: &()| callback(a, b, c, d)),
        o1,
        o2,
        o3,
        o4,
        (),
    );
}

pub fn use_effect5<F, Destructor, T1, T2, T3, T4, T5>(
    callback: Box<F>,
    o1: T1,
    o2: T2,
    o3: T3,
    o4: T4,
    o5: T5,
) where
    F: FnOnce(&T1, &T2, &T3, &T4, &T5) -> Destructor,
    Destructor: FnOnce() + 'static,
    T1: PartialEq + 'static,
    T2: PartialEq + 'static,
    T3: PartialEq + 'static,
    T4: PartialEq + 'static,
    T5: PartialEq + 'static,
{
    struct UseEffectState<T1, T2, T3, T4, T5, Destructor> {
        o1: Rc<T1>,
        o2: Rc<T2>,
        o3: Rc<T3>,
        o4: Rc<T4>,
        o5: Rc<T5>,
        destructor: Option<Box<Destructor>>,
    }
    let o1 = Rc::new(o1);
    let o2 = Rc::new(o2);
    let o3 = Rc::new(o3);
    let o4 = Rc::new(o4);
    let o5 = Rc::new(o5);
    let o1_c = o1.clone();
    let o2_c = o2.clone();
    let o3_c = o3.clone();
    let o4_c = o4.clone();
    let o5_c = o5.clone();
    use_hook(
        move |state: &mut UseEffectState<T1, T2, T3, T4, T5, Destructor>, hook_update| {
            let mut should_update = !(*state.o1 == *o1
                && *state.o2 == *o2
                && *state.o3 == *o3
                && *state.o4 == *o4
                && *state.o5 == *o5);

            if should_update {
                if let Some(de) = state.destructor.take() {
                    de();
                }
                let new_destructor = callback(
                    o1.borrow(),
                    o2.borrow(),
                    o3.borrow(),
                    o4.borrow(),
                    o5.borrow(),
                );
                state.o1 = o1.clone();
                state.o2 = o2.clone();
                state.o3 = o3.clone();
                state.o4 = o4.clone();
                state.o5 = o5.clone();
                state.destructor.replace(Box::new(new_destructor));
            } else if state.destructor.is_none() {
                should_update = true;
                state.destructor.replace(Box::new(callback(
                    state.o1.borrow(),
                    state.o2.borrow(),
                    state.o3.borrow(),
                    state.o4.borrow(),
                    state.o5.borrow(),
                )));
            }
            return move || {
                if should_update {
                    hook_update(move |_: &mut UseEffectState<T1, T2, T3, T4, T5, Destructor>| true)
                }
            };
        },
        || UseEffectState {
            o1: o1_c,
            o2: o2_c,
            o3: o3_c,
            o4: o4_c,
            o5: o5_c,
            destructor: None,
        },
    )();
}

pub fn use_hook<InternalHookState, HookRunner, R, InitialStateProvider, PretriggerChange: 'static>(
    hook_runner: HookRunner,
    initial_state_producer: InitialStateProvider,
) -> R
where
    HookRunner: FnOnce(&mut InternalHookState, Box<dyn Fn(PretriggerChange)>) -> R,
    InternalHookState: 'static,
    InitialStateProvider: FnOnce() -> InternalHookState,
    PretriggerChange: FnOnce(&mut InternalHookState) -> bool,
{
    // Extract current hook
    let (hook, process_message) = CURRENT_HOOK.with(|hook_state_holder| {
        let hook_state_holder = hook_state_holder.try_borrow_mut();
        let mut hook_state_holder = hook_state_holder.expect("Nested hooks not supported");
        let mut hook_state = hook_state_holder
            .as_mut()
            .expect("No current hook. Hooks can only be called inside functional components");

        // Determine which hook position we're at and increment for the next hook
        let hook_pos = hook_state.counter;
        hook_state.counter += 1;

        // Initialize hook if this is the first call
        if hook_pos >= hook_state.hooks.len() {
            let initial_state = Rc::new(RefCell::new(initial_state_producer()));
            hook_state.hooks.push(initial_state);
        }

        let hook = hook_state.hooks[hook_pos].clone();

        return (hook, hook_state.process_message.clone());
    });

    let trigger = {
        let hook = hook.clone();
        Box::new(move |pretrigger_change: PretriggerChange| {
            let hook = hook.clone();
            process_message(Box::new(move || {
                let mut hook = hook.borrow_mut();
                let hook = hook.downcast_mut::<InternalHookState>();
                let hook = hook.expect(
                    "Incompatible hook type. Hooks must always be called in the same order",
                );
                pretrigger_change(hook)
            }));
        })
    };
    let mut hook = hook.borrow_mut();
    let hook = hook.downcast_mut::<InternalHookState>();
    let mut hook =
        hook.expect("Incompatible hook type. Hooks must always be called in the same order");

    // Execute the actual hook closure we were given. Let it mutate the hook state and let
    // it create a callback that takes the mutable hook state.
    hook_runner(&mut hook, trigger)
}
