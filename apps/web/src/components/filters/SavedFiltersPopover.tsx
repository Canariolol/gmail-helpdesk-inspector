import { Bookmark, Check, Save, Trash2 } from "lucide-react";
import { useState } from "react";
import type { FilterPreset } from "../../api/types";
import { FilterPopover } from "./FilterPopover";

type Props = {
  presets: FilterPreset[];
  selectedId: string;
  onApply: (preset: FilterPreset) => void;
  onClear: () => void;
  onSave?: (name: string) => void;
  onDelete?: (id: string) => void;
};

export function SavedFiltersPopover({ presets, selectedId, onApply, onClear, onSave, onDelete }: Props) {
  const [name, setName] = useState("");
  const selected = presets.find((preset) => preset.id === selectedId);

  function handleSave() {
    const trimmed = name.trim();
    if (!trimmed || !onSave) return;
    onSave(trimmed);
    setName("");
  }

  const triggerContent = (
    <>
      <Bookmark size={16} className="fpop-icon" />
      <span className="fpop-value">{selected ? selected.name : "Filtros guardados"}</span>
    </>
  );

  return (
    <FilterPopover
      triggerLabel="Filtros guardados"
      triggerContent={triggerContent}
      panelClassName="presets-panel"
      align="end"
      isActive={Boolean(selected)}
    >
      {(close) => (
        <div className="presets">
          <div className="presets-list">
            <button
              type="button"
              className={`presets-item presets-item-main${!selected ? " is-active" : ""}`}
              onClick={() => {
                onClear();
                close();
              }}
            >
              <span className="presets-item-name">Sin preset</span>
              {!selected && <Check size={15} />}
            </button>

            {presets.map((preset) => (
              <div key={preset.id} className={`presets-row${preset.id === selectedId ? " is-active" : ""}`}>
                <button
                  type="button"
                  className="presets-item presets-item-main"
                  onClick={() => {
                    onApply(preset);
                    close();
                  }}
                >
                  <span className="presets-item-name">
                    {preset.name}
                    {preset.is_default ? " · por defecto" : ""}
                  </span>
                  {preset.id === selectedId && <Check size={15} />}
                </button>
                {onDelete && (
                  <button
                    type="button"
                    className="presets-del"
                    title="Eliminar filtro"
                    aria-label={`Eliminar ${preset.name}`}
                    onClick={() => onDelete(preset.id)}
                  >
                    <Trash2 size={14} />
                  </button>
                )}
              </div>
            ))}

            {presets.length === 0 && <p className="presets-empty">Aún no guardas filtros.</p>}
          </div>

          {onSave && (
            <div className="presets-save">
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    handleSave();
                  }
                }}
                placeholder="Nombre del filtro nuevo"
                aria-label="Nombre del filtro nuevo"
              />
              <button
                type="button"
                className="presets-save-btn"
                onClick={handleSave}
                disabled={!name.trim()}
                title="Guardar filtro actual"
              >
                <Save size={15} />
                Guardar
              </button>
            </div>
          )}
        </div>
      )}
    </FilterPopover>
  );
}
