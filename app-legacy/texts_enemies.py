"""Texts of the enemy aggression options (alert radius, aggression). Merged into the app's tables on import, so
app-legacy/i18n.py does not change."""
from i18n import T

_TXT = {
    "en": dict(
        aggro_title="Aggression",
        enemy_alert="Alert radius",
        enemy_alert_help="How far an enemy that spots you alerts the others (x2 = twice as far).",
        enemy_aggro="Aggression",
        aggro_0="Normal", aggro_1="Aggressive", aggro_2="Maximum",
        enemy_aggro_help="Aggressive: shorter pause between attacks of different enemies, and they chase you 1.5x "
                         "farther. Maximum: they all attack at once, chase you twice as far and fight even when "
                         "they are not on screen. Bosses are not changed.",
    ),
    "es": dict(
        aggro_title="Agresividad",
        enemy_alert="Radio de alerta",
        enemy_alert_help="A qué distancia un enemigo que te ve avisa a los demás (x2 = el doble de lejos).",
        enemy_aggro="Agresividad",
        aggro_0="Normal", aggro_1="Agresiva", aggro_2="Máxima",
        enemy_aggro_help="Agresiva: menos pausa entre los ataques de distintos enemigos y te persiguen 1,5 veces más "
                         "lejos. Máxima: atacan todos a la vez, te persiguen el doble de lejos y pelean aunque no "
                         "estén en pantalla. Los jefes no cambian.",
    ),
    "pt": dict(
        aggro_title="Agressividade",
        enemy_alert="Raio de alerta",
        enemy_alert_help="A que distância um inimigo que te vê avisa os outros (x2 = o dobro da distância).",
        enemy_aggro="Agressividade",
        aggro_0="Normal", aggro_1="Agressiva", aggro_2="Máxima",
        enemy_aggro_help="Agressiva: menos pausa entre os ataques de inimigos diferentes e te perseguem 1,5 vez mais "
                         "longe. Máxima: atacam todos ao mesmo tempo, te perseguem o dobro e lutam mesmo fora da "
                         "tela. Os chefes não mudam.",
    ),
    "fr": dict(
        aggro_title="Agressivité",
        enemy_alert="Rayon d'alerte",
        enemy_alert_help="À quelle distance un ennemi qui vous voit prévient les autres (x2 = deux fois plus loin).",
        enemy_aggro="Agressivité",
        aggro_0="Normale", aggro_1="Agressive", aggro_2="Maximale",
        enemy_aggro_help="Agressive : moins de pause entre les attaques de différents ennemis, et ils vous poursuivent "
                         "1,5 fois plus loin. Maximale : ils attaquent tous en même temps, vous poursuivent deux fois "
                         "plus loin et se battent même hors de l'écran. Les boss ne changent pas.",
    ),
    "de": dict(
        aggro_title="Aggressivität",
        enemy_alert="Alarmradius",
        enemy_alert_help="Wie weit ein Gegner, der dich sieht, die anderen alarmiert (x2 = doppelt so weit).",
        enemy_aggro="Aggressivität",
        aggro_0="Normal", aggro_1="Aggressiv", aggro_2="Maximal",
        enemy_aggro_help="Aggressiv: kürzere Pause zwischen Angriffen verschiedener Gegner, sie verfolgen dich 1,5-mal "
                         "so weit. Maximal: alle greifen gleichzeitig an, verfolgen dich doppelt so weit und kämpfen "
                         "auch außerhalb des Bildschirms. Bosse bleiben unverändert.",
    ),
    "it": dict(
        aggro_title="Aggressività",
        enemy_alert="Raggio di allerta",
        enemy_alert_help="A che distanza un nemico che ti vede avvisa gli altri (x2 = il doppio).",
        enemy_aggro="Aggressività",
        aggro_0="Normale", aggro_1="Aggressiva", aggro_2="Massima",
        enemy_aggro_help="Aggressiva: meno pausa tra gli attacchi di nemici diversi e ti inseguono 1,5 volte più "
                         "lontano. Massima: attaccano tutti insieme, ti inseguono il doppio e combattono anche fuori "
                         "dallo schermo. I boss non cambiano.",
    ),
}
for _lang, _t in _TXT.items():
    if _lang in T:
        T[_lang].update(_t)
