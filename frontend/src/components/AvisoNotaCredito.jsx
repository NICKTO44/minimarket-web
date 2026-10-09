import { useState } from 'react';
import { imprimirNotaCredito } from '../utils/notaCredito';
import './AvisoNotaCredito.css';

/**
 * Resultado de la nota de crédito que sale con una devolución o un cambio de
 * prenda (emisión directa): { nota: {id, serie, numero, estado, mensaje, total,
 * documento_afectado, hash} | null, aviso: texto | null }.
 */
export default function AvisoNotaCredito({ resultado }) {
  const [error, setError] = useState('');
  const { nota, aviso } = resultado;
  const aceptada = nota?.estado === 'ACEPTADO';
  const imprimir = (formato) => {
    setError('');
    imprimirNotaCredito(nota, formato).catch((e) => setError(e.message));
  };

  if (!nota) {
    return <div className="anc anc-aviso">{aviso}</div>;
  }
  return (
    <div className={`anc ${aceptada ? 'anc-ok' : 'anc-aviso'}`}>
      <div>
        <strong>
          Nota de crédito {nota.serie}-{nota.numero} por S/ {nota.total.toFixed(2)}
        </strong>{' '}
        (corrige {nota.documento_afectado}):{' '}
        {aceptada
          ? 'aceptada por SUNAT.'
          : nota.estado === 'PENDIENTE'
            ? 'quedó pendiente; el sistema la reenvía solo.'
            : `${nota.estado.toLowerCase()}. ${nota.mensaje}`}
      </div>
      {aceptada && (
        <div className="anc-botones">
          <button type="button" onClick={() => imprimir('ticket')}>
            🖨 Ticket 80 mm
          </button>
          <button type="button" onClick={() => imprimir('a4')}>
            Hoja A4
          </button>
        </div>
      )}
      {error && <div className="anc-error">{error}</div>}
    </div>
  );
}
