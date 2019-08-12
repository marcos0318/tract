use super::codegen::Codegen;

use super::*;

#[derive(Debug, Clone, Default)]
pub struct Typed {
    pub skip: usize,
    pub body: TypedModel,
    decluttered: bool,
    pub input_mapping: Vec<InputMapping<TDim>>,
    pub output_mapping: Vec<OutputMapping<TDim, TDim>>,
}

impl Typed {
    pub fn to_codegen_op(&self) -> TractResult<Codegen> {
        trace!("Optimizing(Codegen) inner model");
        let plan = SimplePlan::new(self.body.clone().into_optimized()?)?;
        trace!("Optimizing(Codegen) inner model done");
        let input_mapping = self
            .input_mapping
            .iter()
            .map(|im| {
                Ok(match im {
                    InputMapping::Scan { axis, slot, chunk } => InputMapping::Scan {
                        axis: *axis,
                        slot: *slot,
                        chunk: chunk.to_integer()? as usize,
                    },
                    InputMapping::Full { slot } => InputMapping::Full { slot: *slot },
                    InputMapping::State { initializer } => {
                        InputMapping::State { initializer: initializer.clone() }
                    }
                })
            })
            .collect::<TractResult<_>>()?;

        let output_mapping = self
            .output_mapping
            .iter()
            .map(|im| {
                Ok(match im {
                    OutputMapping::Scan { axis, slot, chunk, full_dim_hint } => {
                        OutputMapping::Scan {
                            axis: *axis,
                            slot: *slot,
                            chunk: chunk.to_integer()? as usize,
                            full_dim_hint: full_dim_hint.clone(),
                        }
                    }
                    OutputMapping::State { slot } => OutputMapping::State { slot: *slot },
                })
            })
            .collect::<TractResult<_>>()?;

        Ok(Codegen::new(self.skip, Arc::new(plan), input_mapping, output_mapping))
    }

    pub fn new(
        body: TypedModel,
        input_mapping: Vec<InputMapping<TDim>>,
        output_mapping: Vec<OutputMapping<TDim, TDim>>,
    ) -> Typed {
        Typed { skip: 0, body, decluttered: false, input_mapping, output_mapping }
    }
}

impl Op for Typed {
    fn name(&self) -> Cow<str> {
        "Scan::Typed".into()
    }

    fn info(&self) -> TractResult<Vec<String>> {
        let mut lines = vec![];
        for (ix, im) in self.input_mapping.iter().enumerate() {
            lines.push(format!("Model input  #{}: {:?}", ix, im));
        }
        for (ix, om) in self.output_mapping.iter().enumerate() {
            lines.push(format!("Model output #{}: {:?}", ix, om));
        }
        Ok(lines)
    }

    fn nested_models(&self) -> Vec<(Cow<str>, &dyn Model)> {
        vec![("loop".into(), &self.body)]
    }

    fn declutter(
        &self,
        model: &TypedModel,
        node: &TypedNode,
    ) -> TractResult<Option<TypedModelPatch>> {
        if !self.decluttered {
            let mut new = self.clone();
            new.body = self.body.clone().declutter()?;
            new.decluttered = true;
            return Ok(Some(TypedModelPatch::replace_single_op(model, node, &node.inputs, new)?));
        }
        Ok(None)
    }

    fn pulsify(
        &self,
        _source: &NormalizedModel,
        node: &NormalizedNode,
        target: &mut PulsedModel,
        mapping: &HashMap<OutletId, OutletId>,
    ) -> TractResult<TVec<OutletId>> {
        if node.inputs.len() > 1 || node.outputs.len() > 1 {
            bail!("Scan pulsificiaton limited to single streaming input and output case");
        }
        let input = mapping[&node.inputs[0]];
        let input_fact = target.outlet_fact(input)?;
        let (_slot, axis, _chunk) = self
            .input_mapping
            .iter()
            .filter_map(InputMapping::as_scan)
            .find(|mapping| mapping.0 == 0)
            .unwrap();
        if input_fact.axis != axis {
            bail!("Scan pulsification limited to scanning axis");
        }

        let mut output_fact = crate::pulse::PulsedTensorFact::from_tensor_fact_pulse(
            &node.outputs[0].fact,
            input_fact.pulse(),
        )?;
        output_fact.delay = input_fact.delay;
        let mut op = self.clone();
        op.skip = input_fact.delay;
        let id = target.chain_after(input, &*node.name, op, tvec!(output_fact))?;
        Ok(tvec!(OutletId::new(id, 0)))
    }

    fn codegen(
        &self,
        model: &TypedModel,
        node: &TypedNode,
    ) -> TractResult<Option<TypedModelPatch>> {
        Ok(Some(TypedModelPatch::replace_single_op(
            &model,
            node,
            &node.inputs,
            self.to_codegen_op()?,
        )?))
    }
}

impl StatefullOp for Typed {
    fn state(
        &self,
        session: &mut SessionState,
        node_id: usize,
    ) -> TractResult<Option<Box<dyn OpState>>> {
        self.to_codegen_op()?.state(session, node_id)
    }
}