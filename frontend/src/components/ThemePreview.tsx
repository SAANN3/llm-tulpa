import { useState } from 'react'

import { Button, Checkbox, Div, Icon, Input, Label, RadioButton, Select, TextField, ToggleSwitch } from './primitives'
import { useTheme } from '../context/useTheme'
import type { ThemeName } from '../themes'

/** Renders every primitive once, wired to local state so you can actually interact with them — for eyeballing what the active theme's colors/CSS do to them. */
export function ThemePreview() {
  const [inputText, setInputText] = useState('Input')
  const [textFieldText, setTextFieldText] = useState('TextField')
  const [selected, setSelected] = useState('One')
  const [radioValue, setRadioValue] = useState('A')
  const [checked, setChecked] = useState(true)
  const [toggled, setToggled] = useState(true)
  const {themeName, setThemeName, themeNames} = useTheme()

  return (
    <Div className="vbox" style={{ gap: 4 }}>
      <Select values={themeNames as string[]} onChosen={(x) => setThemeName(x as ThemeName)} selected={themeName}/>
      <Label variant="secondary" text="Select theme that will be used for ui." />
      <Div
        className="panel"
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(3, minmax(150px, 1fr))',
          gap: 16,
          padding: 16,
          justifyItems: 'center',
          alignItems: 'center',
        }}
      >
        <Label text="Label" />
        <Button text="Button" onClicked={() => { }} />
        <Input text={inputText} onChanged={setInputText} />
        <TextField text={textFieldText} onChanged={setTextFieldText} />
        <Select values={['One', 'Two', 'Three']} selected={selected} onChosen={setSelected} />
        <Div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <RadioButton name="preview-radio" value="A" checked={radioValue === 'A'} onChanged={setRadioValue} />
          <RadioButton name="preview-radio" value="B" checked={radioValue === 'B'} onChanged={setRadioValue} />
        </Div>
        <Checkbox toggled={checked} onToggled={setChecked} />
        <ToggleSwitch toggled={toggled} onToggled={setToggled} />
        <Icon src="/favicon.svg" />
      </Div>
    </Div>
  )
}
