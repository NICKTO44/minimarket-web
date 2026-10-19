import { useState } from 'react';
import { PAPEL_58, PAPEL_80, guardarPapelTicket, papelTicket } from '../../utils/papelTicket';
import './ConfiguracionDetraccion.css';
import './ConfiguracionImpresora.css';

const OPCIONES = [
  { valor: PAPEL_80, titulo: '80 mm', detalle: 'Ticketera grande (la más común en caja).' },
  { valor: PAPEL_58, titulo: '58 mm', detalle: 'Ticketera chica o portátil (Bluetooth).' },
];

/**
 * Configuración → Impresora de tickets. Ancho del papel de la ticketera de
 * ESTE equipo (utils/papelTicket.js): se aplica al instante y no se guarda
 * en el negocio, porque cada celular o computadora tiene su impresora.
 */
export default function ConfiguracionImpresora({ onMensaje }) {
  const [papel, setPapel] = useState(papelTicket);

  const elegir = (valor) => {
    guardarPapelTicket(valor);
    setPapel(valor);
    onMensaje({ tipo: 'exito', texto: `Listo: en este equipo los tickets se imprimen en papel de ${valor} mm.` });
  };

  return (
    <div className="cfg-card det-card">
      <div className="det-cabecera">
        <h3 className="cfg-subtitulo-seccion">Impresora de tickets</h3>
      </div>
      <p className="cfg-nota-moneda">
        Ancho del papel de la ticketera conectada a <strong>este equipo</strong>. Se aplica a boletas, facturas, notas
        de venta, comandas y precuentas. Cada celular o computadora guarda su propia opción: elígela en cada equipo
        que imprima.
      </p>
      <div className="imp-opciones" role="radiogroup" aria-label="Ancho del papel">
        {OPCIONES.map((o) => (
          <button
            key={o.valor}
            type="button"
            role="radio"
            aria-checked={papel === o.valor}
            className={`imp-opcion${papel === o.valor ? ' imp-opcion-activa' : ''}`}
            onClick={() => elegir(o.valor)}
          >
            <span className="imp-opcion-titulo">{o.titulo}</span>
            <span className="imp-opcion-detalle">{o.detalle}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
