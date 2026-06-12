type Props = {
  value: string;
  onChange: (value: string) => void;
};

export function ThreadFilterSelect({ value, onChange }: Props) {
  return (
    <select className="filter-select" value={value} onChange={(event) => onChange(event.target.value)}>
      <option value="all">Todos</option>
      <option value="valid_client_request">Solicitudes válidas</option>
      <option value="answered">Respondidas</option>
      <option value="unanswered">Sin respuesta</option>
      <option value="review">Revisión pendiente</option>
      <option value="misc">Ignorados</option>
    </select>
  );
}
