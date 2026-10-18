import { CONDICIONES_VENTA, FORMAS_FARMACEUTICAS } from '../../utils/medicamentos';

// Sección "Ficha del medicamento" del formulario de producto (módulo
// FICHA_MEDICAMENTO). Un producto sin marcar "Es un medicamento" es un
// producto común: en una botica también se venden pañales o golosinas.
// `principios`: principios activos ya usados en el negocio, para sugerir.
export default function FichaMedicamento({ campos, onCambiar, principios = [] }) {
  const cambiar = (campo, valor) => onCambiar({ ...campos, [campo]: valor });

  return (
    <div className="inv-ficha-caja">
      <label className="inv-ficha-interruptor">
        <input
          type="checkbox"
          checked={campos.es_medicamento}
          onChange={(e) => cambiar('es_medicamento', e.target.checked)}
        />
        <span>
          <strong>Es un medicamento</strong>
          <small>Principio activo, concentración, laboratorio, registro sanitario y si pide receta.</small>
        </span>
      </label>

      {campos.es_medicamento && (
        <div className="inv-ficha-campos">
          <div className="inv-campo inv-ficha-ancho">
            <label>Principio activo (DCI)</label>
            <input
              value={campos.principio_activo}
              maxLength={150}
              list="inv-principios-activos"
              placeholder="Paracetamol, Amoxicilina, Paracetamol + Cafeína…"
              onChange={(e) => cambiar('principio_activo', e.target.value)}
            />
            <datalist id="inv-principios-activos">
              {principios.map((p) => (
                <option key={p} value={p} />
              ))}
            </datalist>
          </div>
          <div className="inv-campo">
            <label>Concentración</label>
            <input
              value={campos.concentracion}
              maxLength={40}
              placeholder="500 mg, 250 mg/5 ml"
              onChange={(e) => cambiar('concentracion', e.target.value)}
            />
          </div>
          <div className="inv-campo">
            <label>Forma farmacéutica</label>
            <select value={campos.forma_farmaceutica} onChange={(e) => cambiar('forma_farmaceutica', e.target.value)}>
              <option value="">Sin indicar</option>
              {FORMAS_FARMACEUTICAS.map((f) => (
                <option key={f.valor} value={f.valor}>
                  {f.label}
                </option>
              ))}
            </select>
          </div>
          <div className="inv-campo">
            <label>Laboratorio</label>
            <input value={campos.laboratorio} maxLength={80} onChange={(e) => cambiar('laboratorio', e.target.value)} />
          </div>
          <div className="inv-campo">
            <label>Registro sanitario</label>
            <input
              value={campos.registro_sanitario}
              maxLength={30}
              placeholder="Como figura en el envase"
              onChange={(e) => cambiar('registro_sanitario', e.target.value.toUpperCase())}
            />
          </div>
          <div className="inv-campo inv-ficha-ancho">
            <label>Condición de venta</label>
            <select value={campos.condicion_venta} onChange={(e) => cambiar('condicion_venta', e.target.value)}>
              {CONDICIONES_VENTA.map((c) => (
                <option key={c.valor} value={c.valor}>
                  {c.label}
                </option>
              ))}
            </select>
            {campos.condicion_venta !== 'SIN_RECETA' && (
              <small className="inv-ficha-ayuda">
                Usa la que figura en el registro sanitario del producto. Con el control de recetas encendido, el punto
                de venta la pide antes de cobrar.
              </small>
            )}
          </div>
          <label className="inv-ficha-generico inv-ficha-ancho">
            <input type="checkbox" checked={campos.es_generico} onChange={(e) => cambiar('es_generico', e.target.checked)} />
            Es genérico (se vende por su principio activo, sin marca)
          </label>
        </div>
      )}
    </div>
  );
}
