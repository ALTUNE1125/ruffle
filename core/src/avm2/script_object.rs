//! Default AVM2 object impl

use crate::avm2::function::Executable;
use crate::avm2::names::QName;
use crate::avm2::object::{Object, ObjectPtr, TObject};
use crate::avm2::property::Property;
use crate::avm2::return_value::ReturnValue;
use crate::avm2::value::Value;
use crate::avm2::{Avm2, Error};
use crate::context::UpdateContext;
use gc_arena::{Collect, GcCell, MutationContext};
use std::collections::HashMap;
use std::fmt::Debug;

/// Default implementation of `avm2::Object`.
#[derive(Clone, Collect, Debug, Copy)]
#[collect(no_drop)]
pub struct ScriptObject<'gc>(GcCell<'gc, ScriptObjectData<'gc>>);

#[derive(Clone, Collect, Debug)]
#[collect(no_drop)]
pub struct ScriptObjectData<'gc> {
    /// Properties stored on this object.
    values: HashMap<QName, Property<'gc>>,

    /// Slots stored on this object.
    slots: Vec<Value<'gc>>,

    /// Implicit prototype (or declared base class) of this script object.
    proto: Option<Object<'gc>>,
}

impl<'gc> TObject<'gc> for ScriptObject<'gc> {
    fn get_property(
        self,
        name: &QName,
        avm: &mut Avm2<'gc>,
        context: &mut UpdateContext<'_, 'gc, '_>,
    ) -> Result<ReturnValue<'gc>, Error> {
        self.0.read().get_property(name, avm, context, self.into())
    }

    fn set_property(
        self,
        name: &QName,
        value: Value<'gc>,
        avm: &mut Avm2<'gc>,
        context: &mut UpdateContext<'_, 'gc, '_>,
    ) -> Result<(), Error> {
        self.0
            .write(context.gc_context)
            .set_property(name, value, avm, context, self.into())
    }

    fn get_slot(self, id: u32) -> Result<Value<'gc>, Error> {
        self.0.read().get_slot(id)
    }

    fn set_slot(
        self,
        id: u32,
        value: Value<'gc>,
        mc: MutationContext<'gc, '_>,
    ) -> Result<(), Error> {
        self.0.write(mc).set_slot(id, value, mc)
    }

    fn has_property(self, name: &QName) -> bool {
        self.0.read().has_property(name)
    }

    fn proto(&self) -> Option<Object<'gc>> {
        self.0.read().proto
    }

    fn as_ptr(&self) -> *const ObjectPtr {
        self.0.as_ptr() as *const ObjectPtr
    }

    fn construct(
        &self,
        _avm: &mut Avm2<'gc>,
        context: &mut UpdateContext<'_, 'gc, '_>,
        _args: &[Value<'gc>],
    ) -> Result<Object<'gc>, Error> {
        let this: Object<'gc> = Object::ScriptObject(*self);
        Ok(ScriptObject::object(context.gc_context, this))
    }

    fn install_method(&mut self, mc: MutationContext<'gc, '_>, name: QName, function: Object<'gc>) {
        self.0.write(mc).install_method(name, function)
    }

    fn install_getter(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName,
        function: Executable<'gc>,
    ) -> Result<(), Error> {
        self.0.write(mc).install_getter(name, function)
    }

    fn install_setter(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName,
        function: Executable<'gc>,
    ) -> Result<(), Error> {
        self.0.write(mc).install_setter(name, function)
    }

    fn install_dynamic_property(
        &mut self,
        mc: MutationContext<'gc, '_>,
        name: QName,
        value: Value<'gc>,
    ) -> Result<(), Error> {
        self.0.write(mc).install_dynamic_property(name, value)
    }
}

impl<'gc> ScriptObject<'gc> {
    /// Construct a bare object with no base class.
    ///
    /// This is *not* the same thing as an object literal, which actually does
    /// have a base class: `Object`.
    pub fn bare_object(mc: MutationContext<'gc, '_>) -> Object<'gc> {
        ScriptObject(GcCell::allocate(mc, ScriptObjectData::base_new(None))).into()
    }

    /// Construct an object with a base class.
    pub fn object(mc: MutationContext<'gc, '_>, proto: Object<'gc>) -> Object<'gc> {
        ScriptObject(GcCell::allocate(
            mc,
            ScriptObjectData::base_new(Some(proto)),
        ))
        .into()
    }
}

impl<'gc> ScriptObjectData<'gc> {
    pub fn base_new(proto: Option<Object<'gc>>) -> Self {
        ScriptObjectData {
            values: HashMap::new(),
            slots: Vec::new(),
            proto,
        }
    }

    pub fn get_property(
        &self,
        name: &QName,
        avm: &mut Avm2<'gc>,
        context: &mut UpdateContext<'_, 'gc, '_>,
        this: Object<'gc>,
    ) -> Result<ReturnValue<'gc>, Error> {
        let prop = self.values.get(name);

        if let Some(prop) = prop {
            prop.get(avm, context, this)
        } else {
            Ok(Value::Undefined.into())
        }
    }

    pub fn set_property(
        &mut self,
        name: &QName,
        value: Value<'gc>,
        avm: &mut Avm2<'gc>,
        context: &mut UpdateContext<'_, 'gc, '_>,
        this: Object<'gc>,
    ) -> Result<(), Error> {
        if let Some(prop) = self.values.get_mut(name) {
            prop.set(avm, context, this, value)?;
        } else {
            //TODO: Not all classes are dynamic like this
            self.values
                .insert(name.clone(), Property::new_dynamic_property(value));
        }

        Ok(())
    }

    pub fn get_slot(&self, id: u32) -> Result<Value<'gc>, Error> {
        self.slots
            .get(id as usize)
            .cloned()
            .ok_or_else(|| format!("Slot index {} out of bounds!", id).into())
    }

    /// Set a slot by it's index.
    pub fn set_slot(
        &mut self,
        id: u32,
        value: Value<'gc>,
        _mc: MutationContext<'gc, '_>,
    ) -> Result<(), Error> {
        if let Some(slot) = self.slots.get_mut(id as usize) {
            *slot = value;

            Ok(())
        } else {
            Err(format!("Slot index {} out of bounds!", id).into())
        }
    }

    pub fn has_property(&self, name: &QName) -> bool {
        self.values.get(name).is_some()
    }

    pub fn proto(&self) -> Option<Object<'gc>> {
        self.proto
    }

    /// Install a method into the object.
    pub fn install_method(&mut self, name: QName, function: Object<'gc>) {
        self.values.insert(name, Property::new_method(function));
    }

    /// Install a getter into the object.
    ///
    /// This is a little more complicated than methods, since virtual property
    /// slots can be installed in two parts. Thus, we need to support
    /// installing them in either order.
    pub fn install_getter(&mut self, name: QName, function: Executable<'gc>) -> Result<(), Error> {
        if !self.values.contains_key(&name) {
            self.values.insert(name.clone(), Property::new_virtual());
        }

        self.values
            .get_mut(&name)
            .unwrap()
            .install_virtual_getter(function)
    }

    /// Install a setter into the object.
    ///
    /// This is a little more complicated than methods, since virtual property
    /// slots can be installed in two parts. Thus, we need to support
    /// installing them in either order.
    pub fn install_setter(&mut self, name: QName, function: Executable<'gc>) -> Result<(), Error> {
        if !self.values.contains_key(&name) {
            self.values.insert(name.clone(), Property::new_virtual());
        }

        self.values
            .get_mut(&name)
            .unwrap()
            .install_virtual_setter(function)
    }

    pub fn install_dynamic_property(
        &mut self,
        name: QName,
        value: Value<'gc>,
    ) -> Result<(), Error> {
        self.values
            .insert(name, Property::new_dynamic_property(value));

        Ok(())
    }
}
