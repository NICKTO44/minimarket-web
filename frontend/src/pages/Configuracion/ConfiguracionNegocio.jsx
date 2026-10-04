import { useState } from 'react';
import { api } from '../../api/api';
import { confirmar } from '../../utils/confirmar';
import { MODULO_MESAS, MODULOS, RUBROS, rubroDe } from '../../utils/rubros';
import './ConfiguracionNegocio.css';

function nombresModulos(valores) {
  const nombres = MODULOS.filter((m) => valores.includes(m.valor)).map((m) => m.label);
  return nombres.length ? nombres.join(' y ') : 'ninguno adicional';
}

/**
 * Configuración → Rubro y módulos. El rubro es una plantilla (módulos,
 * unidades sugeridas y nombres de pantalla); los módulos se pueden encender
 * o apagar por separado. Lo universal (ventas, caja, productos, clientes,
 * reportes) no depende de nada de esto.
 */
export default function ConfiguracionNegocio({ rubro, modulos, onCambiado, onMensaje }) {
  const [guardando, setGuardando] = useState(false);
  // La lista de módulos es larga: se abre solo cuando se quiere cambiar algo,
  // para que no empuje hacia abajo los datos del negocio.
  const [verModulos, setVerModulos] = useState(false);
  const encendidos = MODULOS.filter((m) => modulos.includes(m.valor));

  const guardar = async (nuevoRubro, nuevosModulos, textoExito) => {
    setGuardando(true);
    onMensaje(null);
    try {
      const resultado = await api.negocioGuardar(nuevoRubro, nuevosModulos);
      onCambiado(resultado);
      onMensaje({ tipo: 'exito', texto: textoExito });
    } catch (e) {
      onMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  const elegirRubro = async (nuevo) => {
    if (nuevo.valor === rubro || guardando) return;
    const ok = await confirmar({
      titulo: `¿Cambiar el rubro a ${nuevo.label}?`,
      mensaje: `Quedarán encendidos los módulos de ese rubro: ${nombresModulos(nuevo.modulos)}. Tus productos, ventas, caja y clientes no se tocan; no se borra nada. Las unidades sugeridas del rubro las aplicas en la pestaña "Unidades".`,
      textoConfirmar: 'Cambiar rubro',
      icono: 'aviso',
    });
    if (ok) guardar(nuevo.valor, nuevo.modulos, `Rubro cambiado a ${nuevo.label}.`);
  };

  const alternarModulo = async (modulo) => {
    if (guardando) return;
    const encender = !modulos.includes(modulo.valor);
    if (modulo.valor === MODULO_MESAS) {
      const ok = await confirmar({
        titulo: encender ? '¿Activar atención en mesas?' : '¿Desactivar atención en mesas?',
        mensaje: encender
          ? 'Se agrega "Mesas" al menú, los roles Mesero y Barra/Cocina, la carta de hoy y las opciones de productos. Tus ventas, caja e inventario siguen igual. Si no hay mesas, se crean 6 de ejemplo.'
          : 'Se ocultan Mesas, Preparación, la carta de hoy y las opciones de productos. No se borra nada: si lo vuelves a activar, todo sigue ahí.',
        textoConfirmar: encender ? 'Activar' : 'Desactivar',
        icono: 'aviso',
      });
      if (!ok) return;
    }
    const nuevos = encender ? [...modulos, modulo.valor] : modulos.filter((m) => m !== modulo.valor);
    guardar(rubro, nuevos, `${modulo.label}: ${encender ? 'activado' : 'desactivado'}.`);
  };

  return (
    <div className="cfg-card cfg-card-tipo">
      <h3 className="cfg-subtitulo-seccion">Rubro del negocio</h3>
      <p className="cfg-nota-moneda">
        Define qué módulos vienen encendidos, las unidades sugeridas y cómo se llaman algunas pantallas. Puedes
        cambiarlo cuando quieras; no se borra nada.
      </p>
      <div className="cfg-tipos neg-rubros" role="radiogroup" aria-label="Rubro del negocio">
        {RUBROS.map((r) => (
          <button
            key={r.valor}
            type="button"
            role="radio"
            aria-checked={r.valor === rubro}
            className={`cfg-tipo${r.valor === rubro ? ' activo' : ''}`}
            onClick={() => elegirRubro(r)}
            disabled={guardando}
          >
            <r.Icono size={18} />
            <strong>{r.label}</strong>
          </button>
        ))}
      </div>
      <p className="neg-rubro-descripcion">{rubroDe(rubro).descripcion}</p>

      <div className="neg-modulos-cabecera">
        <div>
          <h3 className="cfg-subtitulo-seccion">Módulos</h3>
          <p className="neg-modulos-resumen">
            {encendidos.length > 0 ? `Encendidos: ${encendidos.map((m) => m.label).join(', ')}.` : 'Ninguno adicional encendido.'}
          </p>
        </div>
        <button type="button" className="neg-boton-modulos" onClick={() => setVerModulos((v) => !v)} aria-expanded={verModulos}>
          {verModulos ? 'Ocultar ▴' : 'Ver y cambiar ▾'}
        </button>
      </div>
      {verModulos && (
        <>
      <p className="cfg-nota-moneda">
        Ventas, caja, productos, clientes, proveedores y reportes siempre están. Aquí enciendes solo lo adicional que
        usa tu negocio ({rubroDe(rubro).label}).
      </p>
      <div className="neg-modulos">
        {MODULOS.map((m) => {
          const encendido = modulos.includes(m.valor);
          return (
            <button
              key={m.valor}
              type="button"
              role="switch"
              aria-checked={encendido}
              className={`neg-modulo${encendido ? ' encendido' : ''}`}
              onClick={() => alternarModulo(m)}
              disabled={guardando}
            >
              <span className="neg-interruptor" aria-hidden="true" />
              <span className="neg-modulo-texto">
                <strong>{m.label}</strong>
                <span>{m.descripcion}</span>
              </span>
              <span className="neg-estado">{encendido ? 'Encendido' : 'Apagado'}</span>
            </button>
          );
        })}
      </div>
        </>
      )}
    </div>
  );
}
