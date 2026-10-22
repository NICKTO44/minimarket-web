import { datoReal } from '../utils/validacionRegistro';
import './TarjetaRuc.css';

// Datos del RUC consultado (registro, Configuración y "Completa los datos de
// tu negocio"). Solo muestra: nunca bloquea por el estado o la condición.
const ESTADOS_NORMALES = ['ACTIVO', 'HABIDO'];

export default function TarjetaRuc({ ruc, datos }) {
  const tipo = ruc.startsWith('10') ? 'Persona natural' : ruc.startsWith('20') ? 'Empresa' : 'Contribuyente';
  const comercial = datoReal(datos.nombre_comercial);
  const etiquetas = [datos.estado, datos.condicion].filter(Boolean);
  const fueraDeLoNormal = etiquetas.some((e) => !ESTADOS_NORMALES.includes(e.toUpperCase()));
  return (
    <div className="registro-ruc-tarjeta">
      <span className="registro-ruc-tipo">{tipo} · RUC {ruc}</span>
      <strong className="registro-ruc-razon">{datos.razon_social}</strong>
      {comercial && comercial !== datos.razon_social && (
        <span className="registro-ruc-comercial">{comercial}</span>
      )}
      {etiquetas.length > 0 && (
        <div className="registro-ruc-etiquetas">
          {etiquetas.map((e) => (
            <span key={e} className={`registro-ruc-etiqueta${ESTADOS_NORMALES.includes(e.toUpperCase()) ? '' : ' aviso'}`}>
              {e}
            </span>
          ))}
        </div>
      )}
      {datos.direccion && <span className="registro-ruc-direccion">{datos.direccion}</span>}
      {fueraDeLoNormal && (
        <span className="registro-ruc-nota">Puedes registrarte igual; el estado en SUNAT no impide usar el sistema.</span>
      )}
    </div>
  );
}
