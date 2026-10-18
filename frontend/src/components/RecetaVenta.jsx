import { useEffect, useState } from 'react';
import { FileCheck2 } from 'lucide-react';
import { hoyLima } from '../utils/formato';
import { etiquetaCondicion } from '../utils/medicamentos';
import './CalculadoraPieTablar.css';
import './RecetaVenta.css';

export const RECETA_VACIA = {
  presentada: false,
  medico_nombre: '',
  medico_colegiatura: '',
  paciente_nombre: '',
  paciente_documento: '',
  numero_receta: '',
  fecha_receta: '',
};

const OBLIGATORIOS_RETENIDA = [
  ['medico_nombre', 'médico'],
  ['medico_colegiatura', 'CMP'],
  ['paciente_nombre', 'paciente'],
  ['paciente_documento', 'documento del paciente'],
  ['numero_receta', 'N.° de receta'],
];

/** Lo que se manda al servidor (vacíos como null). */
export function recetaParaEnviar(datos) {
  const t = (v) => v.trim() || null;
  return {
    presentada: datos.presentada,
    medico_nombre: t(datos.medico_nombre),
    medico_colegiatura: t(datos.medico_colegiatura),
    paciente_nombre: t(datos.paciente_nombre),
    paciente_documento: t(datos.paciente_documento),
    numero_receta: t(datos.numero_receta),
    fecha_receta: datos.fecha_receta || null,
  };
}

/**
 * Antes de cobrar medicamentos que piden receta (módulo RECETAS).
 * "Con receta": basta confirmar que el cliente la presentó; los datos son
 * opcionales. "Receta retenida": no se cobra sin médico, CMP, paciente,
 * documento y N.° de receta. El servidor revisa lo mismo.
 */
export default function RecetaVenta({ medicamentos, datosIniciales, onConfirmar, onCerrar }) {
  const [datos, setDatos] = useState(datosIniciales || RECETA_VACIA);
  const retenida = medicamentos.some((m) => m.condicion === 'RECETA_RETENIDA');
  const cambiar = (campo, valor) => setDatos((d) => ({ ...d, [campo]: valor }));

  useEffect(() => {
    const alTeclear = (e) => e.key === 'Escape' && onCerrar(datos);
    window.addEventListener('keydown', alTeclear);
    return () => window.removeEventListener('keydown', alTeclear);
  }, [onCerrar, datos]);

  const faltan = retenida ? OBLIGATORIOS_RETENIDA.filter(([campo]) => !datos[campo].trim()).map(([, nombre]) => nombre) : [];
  const listo = datos.presentada && faltan.length === 0;

  const campo = (clave, etiqueta, props = {}) => {
    const obligatorio = retenida && OBLIGATORIOS_RETENIDA.some(([c]) => c === clave);
    return (
      <label className="pt-campo">
        <span>
          {etiqueta}
          {obligatorio ? ' *' : ''}
        </span>
        <input value={datos[clave]} onChange={(e) => cambiar(clave, e.target.value)} {...props} />
      </label>
    );
  };

  return (
    <div className="pt-velo" onMouseDown={(e) => e.target === e.currentTarget && onCerrar(datos)}>
      <div className="pt-modal rv-modal" role="dialog" aria-modal="true" aria-label="Receta médica">
        <header className="pt-cabecera">
          <span className={`pt-icono${retenida ? ' rv-icono-retenida' : ''}`}>
            <FileCheck2 size={18} />
          </span>
          <div>
            <h2>{retenida ? 'Receta médica retenida' : 'Receta médica'}</h2>
            <p>Estos medicamentos no se venden sin receta.</p>
          </div>
        </header>

        <ul className="rv-lista">
          {medicamentos.map((m) => (
            <li key={m.id}>
              <span>{m.nombre}</span>
              <span className={`rv-etiqueta rv-etiqueta-${m.condicion.toLowerCase()}`}>{etiquetaCondicion(m.condicion)}</span>
            </li>
          ))}
        </ul>

        <label className="rv-confirmar">
          <input type="checkbox" checked={datos.presentada} onChange={(e) => cambiar('presentada', e.target.checked)} autoFocus />
          <span>El cliente presentó la receta médica{retenida ? ' y queda retenida en el establecimiento' : ''}.</span>
        </label>

        <p className="rv-nota">
          {retenida
            ? 'Los campos con * son obligatorios para medicamentos de receta retenida.'
            : 'Los datos de la receta son opcionales, pero quedan en el libro de recetas.'}
        </p>

        <div className="pt-campos rv-campos">
          {campo('medico_nombre', 'Médico', { maxLength: 100 })}
          {campo('medico_colegiatura', 'CMP / colegiatura', { maxLength: 15, inputMode: 'numeric' })}
          {campo('paciente_nombre', 'Paciente', { maxLength: 100 })}
          {campo('paciente_documento', 'DNI del paciente', { maxLength: 15, inputMode: 'numeric' })}
          {campo('numero_receta', 'N.° de receta', { maxLength: 30 })}
          {campo('fecha_receta', 'Fecha de la receta', { type: 'date', max: hoyLima() })}
        </div>

        {datos.presentada && faltan.length > 0 && <p className="pt-aviso">Falta: {faltan.join(', ')}.</p>}

        <div className="pt-acciones">
          <button type="button" className="pt-cancelar" onClick={() => onCerrar(datos)}>
            Volver
          </button>
          <button type="button" className="pt-agregar" disabled={!listo} onClick={() => onConfirmar(datos)}>
            Continuar con el cobro
          </button>
        </div>
      </div>
    </div>
  );
}
