(() => {
  function formFor(action) {
    const form = action?.input_form;
    if (!form || typeof form !== "object") {
      throw new Error("The action does not advertise a bounded input form.");
    }
    if (typeof action.payload_schema !== "string" || action.payload_schema.length === 0) {
      throw new Error("The action has no payload schema.");
    }
    if (form.schema !== action.payload_schema) {
      throw new Error("The action form schema does not match the advertised payload.");
    }
    if (!Array.isArray(form.fields)) {
      throw new Error("The action form has no fields.");
    }
    return form;
  }

  function composePayload(action, values) {
    const form = formFor(action);
    const payload = { schema: form.schema };
    const supplied = values && typeof values === "object" ? values : {};
    for (const name of Object.keys(supplied)) {
      if (!form.fields.some((field) => field && field.name === name)) {
        throw new Error(`The action form does not have a ${name} field.`);
      }
    }
    for (const field of form.fields) {
      if (!field || typeof field.name !== "string" || field.name.length === 0 || field.name === "schema") {
        throw new Error("The action form has an invalid field name.");
      }
      if (!Array.isArray(field.choices) || field.choices.length === 0) {
        throw new Error(`The ${field.label || field.name} field has no choices.`);
      }
      const value = supplied[field.name];
      if ((field.required ?? true) && (typeof value !== "string" || value.length === 0)) {
        throw new Error(`Choose ${field.label || field.name}.`);
      }
      if (value === undefined || value === "") {
        continue;
      }
      if (!field.choices.some((choice) => choice && choice.value === value)) {
        throw new Error(`The selected ${field.label || field.name} value is not advertised.`);
      }
      payload[field.name] = value;
    }
    return payload;
  }

  globalThis.GraphshellActionForm = Object.freeze({ composePayload });
})();
