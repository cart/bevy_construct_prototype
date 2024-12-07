pub use bevy_construct_prototype_macros::Construct;

use bevy_ecs::{
    bundle::DynamicBundle,
    component::{ComponentId, Components, RequiredComponents, StorageType},
    prelude::{Bundle, EntityWorldMut},
    storage::Storages,
    world::error::EntityFetchError,
};
use bevy_ptr::OwningPtr;
use bevy_reflect::{FromType, Reflect};
use std::borrow::Cow;
use thiserror::Error;
use variadics_please::all_tuples;

#[derive(Error, Debug)]
pub enum ConstructError {
    #[error("{0}")]
    Custom(&'static str),
    #[error(transparent)]
    MissingEntity(#[from] EntityFetchError),
    #[error("Resource {type_name} does not exist")]
    MissingResource { type_name: &'static str },
    #[error("Props were invalid: {message}")]
    InvalidProps { message: Cow<'static, str> },
}

pub trait Construct: Sized {
    type Props: Clone + Default;
    fn construct(entity: &mut EntityWorldMut, props: Self::Props) -> Result<Self, ConstructError>;
}

impl<T: Clone + Default> Construct for T {
    type Props = T;

    #[inline]
    fn construct(_entity: &mut EntityWorldMut, props: Self::Props) -> Result<Self, ConstructError> {
        Ok(props)
    }
}

#[derive(Reflect)]
pub enum ConstructProp<C: Construct> {
    Props(C::Props),
    Value(C),
}

impl<C: Construct + Clone> Clone for ConstructProp<C>
where
    C::Props: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Props(arg0) => Self::Props(arg0.clone()),
            Self::Value(arg0) => Self::Value(arg0.clone()),
        }
    }
}

#[derive(Clone)]
pub struct ReflectConstruct {
    pub default_props: fn() -> Box<dyn Reflect>,
    pub construct: fn(
        props: Box<dyn Reflect>,
        &mut EntityWorldMut,
    ) -> Result<Box<dyn Reflect>, ConstructError>,
}

impl<S: Construct> FromType<S> for ReflectConstruct
where
    S: Reflect + Bundle,
    S::Props: Reflect,
{
    fn from_type() -> Self {
        Self {
            default_props: || Box::new(<S::Props as Default>::default()),
            construct: |props, context| {
                let props = props.downcast::<S::Props>().unwrap();
                Ok(Box::new(S::construct(context, *props)?))
            },
        }
    }
}

/// This exists because we cannot impl [`Construct`] for tuples, as that would conflict with the blanket impl of [`Construct`] for [`Default`].
/// This isn't ideal, but given the choice between the nice UX of [`Default`] types being [`Construct`], or the internal Construct behavior of
/// tuples being slightly weirder, we'll take the nice UX.
/// [`ConstructTuple`] implements [`Bundle`], meaning it behaves just like tuple bundle would.  
pub struct ConstructTuple<T>(T);

macro_rules! construct_impl {
    ($($construct: ident),*) => {
        impl<$($construct: Construct),*> Construct for ConstructTuple<($($construct,)*)> {
            type Props = ($($construct::Props,)* );
            #[allow(non_snake_case)]
            fn construct(
                _entity: &mut EntityWorldMut,
                _props: Self::Props,
            ) -> Result<Self, ConstructError> {
                let ($($construct,)*) = _props;
                Ok(ConstructTuple(($(<$construct as Construct>::construct(_entity, $construct)?,)*)))
            }
       }
    }
}

all_tuples!(construct_impl, 0, 12, P);

#[allow(unsafe_code)]
unsafe impl<B: Bundle> Bundle for ConstructTuple<B> {
    fn component_ids(
        components: &mut Components,
        storages: &mut Storages,
        ids: &mut impl FnMut(ComponentId),
    ) {
        B::component_ids(components, storages, ids);
    }

    unsafe fn from_components<T, F>(ctx: &mut T, func: &mut F) -> Self
    where
        // Ensure that the `OwningPtr` is used correctly
        F: for<'a> FnMut(&'a mut T) -> OwningPtr<'a>,
        Self: Sized,
    {
        ConstructTuple(
            // SAFETY: B::from_components has the same constraints as Self::from_components
            unsafe { B::from_components(ctx, func) },
        )
    }

    fn register_required_components(
        components: &mut Components,
        storages: &mut Storages,
        required_components: &mut RequiredComponents,
    ) {
        B::register_required_components(components, storages, required_components);
    }

    fn get_component_ids(_components: &Components, _ids: &mut impl FnMut(Option<ComponentId>)) {
        todo!("Not yet implemented for ConstructTuple")
    }
}

impl<B: Bundle> DynamicBundle for ConstructTuple<B> {
    fn get_components(self, func: &mut impl FnMut(StorageType, OwningPtr<'_>)) {
        self.0.get_components(func);
    }
}
